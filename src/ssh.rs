//! Git over SSH: an in-process SSH server (russh) that runs only
//! git-upload-pack, git-receive-pack and git-upload-archive on an
//! `owner/repo(.git)` path. Access, limits and post-push work are git over
//! HTTP's (`smart_http`): only the transport differs. Off unless
//! GITTER_SSH_ADDR is set.

use std::net::SocketAddr;
use std::path::Path;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicUsize, Ordering};
use std::time::Duration;

use rand::RngCore;
use russh::keys::{HashAlg, PrivateKey, PublicKey};
use russh::server::{Auth, ChannelOpenHandle, Handle, Msg, Session};
use russh::{Channel, ChannelId, ChannelOpenFailure, MethodKind, MethodSet};
use rusqlite::OptionalExtension;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::sync::mpsc;

use crate::AppState;
use crate::auth::{USER_COLS, User};
use crate::db::now;
use crate::error::{AppError, R};
use crate::smart_http as http;

/// From connecting to running a git command: the handshake, signing in and the command.
const START_DEADLINE: Duration = Duration::from_secs(30);
/// How long a connection may stay open after its command ended.
const LINGER: Duration = Duration::from_secs(10);
/// A git command with nothing moving either way for this long is stopped.
const IDLE_SECS: i64 = 120;
/// SSH connections open at once, from everyone (each address also has the git transfer limit).
const MAX_CONNECTIONS: usize = 512;
/// New connections one address may open a minute (before signing in).
const CONNECTS_PER_MIN: u32 = 120;
/// Unknown keys offered from one address per minute before every key from there is refused.
const KEY_FAILS_PER_MIN: u32 = 60;
/// What a clone or fetch may send (its wants and haves).
const FETCH_INPUT_CAP: u64 = 64 * 1024 * 1024;
const HOST_KEY: &str = "ssh_host_ed25519_key";
pub const MAX_KEYS: i64 = 50;

/// Listen on `addr` for git over SSH. `port` is the port clone URLs show, when
/// it isn't the one listened on (a container's mapped port). Returns the address bound.
pub async fn start(st: &AppState, addr: &str, port: Option<u16>) -> anyhow::Result<SocketAddr> {
    let key = host_key(&st.cfg.data_dir)?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let local = listener.local_addr()?;
    let _ = st.ssh_url.set(base_url(&st.cfg.url, port.unwrap_or(local.port())));
    let config = Arc::new(russh::server::Config {
        server_id: russh::SshId::Standard("SSH-2.0-Gitter".into()),
        methods: MethodSet::from(&[MethodKind::PublicKey][..]),
        // Unknown keys are refused quickly: an agent may offer several before the right one.
        auth_rejection_time: Duration::from_millis(200),
        auth_rejection_time_initial: Some(Duration::ZERO),
        keys: vec![key],
        max_auth_attempts: 6,
        inactivity_timeout: Some(Duration::from_secs(3600)),
        keepalive_interval: Some(Duration::from_secs(30)),
        keepalive_max: 3,
        nodelay: true,
        ..Default::default()
    });
    tokio::spawn(accept(st.clone(), listener, config));
    Ok(local)
}

/// `ssh://git@host:port/` or, on port 22, `git@host:` (clone URLs add `owner/repo.git`).
fn base_url(url: &str, port: u16) -> String {
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split('/').next().unwrap_or("");
    let authority = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    // Drop the HTTP port, keeping an IPv6 address's brackets.
    let host = match authority.rfind(':') {
        Some(i) if !authority[i..].contains(']') => &authority[..i],
        _ => authority,
    };
    if port == 22 { format!("git@{host}:") } else { format!("ssh://git@{host}:{port}/") }
}

/// The server's host key, made once in GITTER_DATA.
fn host_key(dir: &Path) -> anyhow::Result<PrivateKey> {
    let path = dir.join(HOST_KEY);
    match std::fs::read_to_string(&path) {
        Ok(pem) => return PrivateKey::from_openssh(pem).map_err(|e| anyhow::anyhow!("{}: {e}", path.display())),
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
        Err(_) => {}
    }
    let mut seed = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut seed);
    let key = PrivateKey::from(russh::keys::ssh_key::private::Ed25519Keypair::from_seed(&seed));
    let pem = key.to_openssh(russh::keys::ssh_key::LineEnding::LF)?;
    match crate::security::write_new_secret(&path, pem.as_bytes()) {
        Ok(()) => Ok(key),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => host_key(dir),
        Err(e) => Err(e.into()),
    }
}

async fn accept(st: AppState, listener: tokio::net::TcpListener, config: Arc<russh::server::Config>) {
    let open = Arc::new(tokio::sync::Semaphore::new(MAX_CONNECTIONS));
    loop {
        let (sock, peer) = match listener.accept().await {
            Ok(x) => x,
            Err(e) => {
                // Out of file descriptors, usually: wait instead of spinning.
                eprintln!("ssh accept: {e}");
                tokio::time::sleep(Duration::from_millis(200)).await;
                continue;
            }
        };
        let Ok(conn) = open.clone().try_acquire_owned() else { continue };
        let ip = peer.ip().to_canonical().to_string();
        // Connections per address per minute; git's transfer limits apply once a command starts.
        if st.limiter.check(&format!("ssh-conn:{}", crate::auth::ip_bucket(&ip)), CONNECTS_PER_MIN, 60).is_err() {
            continue;
        }
        tokio::spawn(connection(st.clone(), config.clone(), sock, ip, conn));
    }
}

async fn connection(
    st: AppState,
    config: Arc<russh::server::Config>,
    sock: tokio::net::TcpStream,
    ip: String,
    _held: tokio::sync::OwnedSemaphorePermit,
) {
    let (started, done, running) = (Arc::new(AtomicBool::new(false)), Arc::new(tokio::sync::Notify::new()), Arc::new(AtomicUsize::new(0)));
    let conn = Conn {
        st,
        ip,
        user: None,
        key: None,
        protocol: None,
        channel: None,
        execed: false,
        started: started.clone(),
        done: done.clone(),
        running: running.clone(),
        stdin: None,
    };
    let Ok(Ok(session)) = tokio::time::timeout(START_DEADLINE, russh::server::run_stream(config, sock, conn)).await else {
        return;
    };
    let handle = session.handle();
    // Sign in and start a command in time; then stay while commands run, and a
    // little after the last (a shared connection may send another), then go.
    let lifetime = async {
        tokio::time::sleep(START_DEADLINE).await;
        if started.load(Ordering::Relaxed) {
            loop {
                while running.load(Ordering::Relaxed) > 0 {
                    done.notified().await;
                }
                if tokio::time::timeout(LINGER, done.notified()).await.is_err() && running.load(Ordering::Relaxed) == 0 {
                    break;
                }
            }
        }
        let _ = handle.disconnect(russh::Disconnect::ByApplication, "Closing the connection".into(), String::new()).await;
        // The session ends on its own now; don't wait for it forever.
        tokio::time::sleep(Duration::from_secs(5)).await;
    };
    tokio::select! {
        _ = session => {}
        _ = lifetime => {}
    }
}

/// One SSH connection: one session channel at a time, each running one git
/// command (a shared connection opens another channel for the next).
struct Conn {
    st: AppState,
    ip: String,
    user: Option<User>,
    /// The key it signed in with, looked up again when a command starts.
    key: Option<PublicKey>,
    /// GIT_PROTOCOL from the client (`version=2`).
    protocol: Option<String>,
    channel: Option<ChannelId>,
    /// The open channel has had its command.
    execed: bool,
    /// Some command has started on this connection.
    started: Arc<AtomicBool>,
    /// Told when a command starts or ends.
    done: Arc<tokio::sync::Notify>,
    running: Arc<AtomicUsize>,
    /// The running command's input.
    stdin: Option<mpsc::Sender<Vec<u8>>>,
}

impl Conn {
    fn fail_key(&self) -> String {
        format!("ssh-key:{}", crate::auth::ip_bucket(&self.ip))
    }
}

/// The account a registered key belongs to (never a suspended one).
async fn key_owner(st: &AppState, key: &PublicKey, touch: bool) -> R<Option<User>> {
    let fp = key.fingerprint(HashAlg::Sha256).to_string();
    st.db
        .call(move |c| {
            let u = c
                .query_row(
                    &format!("SELECT {USER_COLS} FROM users WHERE suspended = 0 AND is_org = 0 AND id = (SELECT user_id FROM ssh_keys WHERE fingerprint = ?1)"),
                    [&fp],
                    User::from_row,
                )
                .optional()?;
            if touch && u.is_some() {
                c.execute("UPDATE ssh_keys SET last_used = ?1 WHERE fingerprint = ?2", rusqlite::params![now(), fp])?;
            }
            Ok(u)
        })
        .await
}

impl russh::server::Handler for Conn {
    type Error = anyhow::Error;

    async fn auth_publickey_offered(&mut self, _user: &str, key: &PublicKey) -> Result<Auth, Self::Error> {
        // A registered key always gets in: someone else's failures on a shared address don't lock it out.
        if key_owner(&self.st, key, false).await.map_err(|e| anyhow::anyhow!(e.msg))?.is_some() {
            return Ok(Auth::Accept);
        }
        if !self.st.limiter.blocked(&self.fail_key(), KEY_FAILS_PER_MIN) {
            self.st.limiter.fail(&self.fail_key(), 60);
        }
        Ok(Auth::reject())
    }

    async fn auth_publickey(&mut self, _user: &str, key: &PublicKey) -> Result<Auth, Self::Error> {
        // The signature checked out: the client holds the key. Any SSH user name works (git@ is usual).
        match key_owner(&self.st, key, true).await.map_err(|e| anyhow::anyhow!(e.msg))? {
            Some(u) => {
                self.user = Some(u);
                self.key = Some(key.clone());
                Ok(Auth::Accept)
            }
            None => Ok(Auth::reject()),
        }
    }

    async fn channel_open_session(&mut self, channel: Channel<Msg>, reply: ChannelOpenHandle, _s: &mut Session) -> Result<(), Self::Error> {
        if self.channel.is_some() {
            reply.reject(ChannelOpenFailure::AdministrativelyProhibited).await;
            return Ok(());
        }
        self.channel = Some(channel.id());
        self.execed = false;
        reply.accept().await;
        // Input arrives through `data`; a kept Channel nobody reads would stall the connection.
        drop(channel);
        Ok(())
    }

    async fn env_request(&mut self, channel: ChannelId, name: &str, value: &str, s: &mut Session) -> Result<(), Self::Error> {
        // Only the git protocol version, as git over HTTP takes it from the Git-Protocol header.
        if name == "GIT_PROTOCOL" && value.len() <= 64 && value.bytes().all(|b| b.is_ascii_alphanumeric() || b"=:._-".contains(&b)) {
            self.protocol = Some(value.to_string());
            s.channel_success(channel)?;
        } else {
            s.channel_failure(channel)?;
        }
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        _w: u32,
        _h: u32,
        _pw: u32,
        _ph: u32,
        _modes: &[(russh::Pty, u32)],
        s: &mut Session,
    ) -> Result<(), Self::Error> {
        s.channel_failure(channel)?;
        Ok(())
    }

    async fn subsystem_request(&mut self, channel: ChannelId, _name: &str, s: &mut Session) -> Result<(), Self::Error> {
        s.channel_failure(channel)?;
        Ok(())
    }

    async fn x11_request(&mut self, channel: ChannelId, _single: bool, _proto: &str, _cookie: &str, _screen: u32, s: &mut Session) -> Result<(), Self::Error> {
        s.channel_failure(channel)?;
        Ok(())
    }

    async fn shell_request(&mut self, channel: ChannelId, s: &mut Session) -> Result<(), Self::Error> {
        let name = self.user.as_ref().map_or("there", |u| u.username.as_str());
        s.channel_success(channel)?;
        let msg = format!("Hi {name}! You've signed in, but Gitter doesn't offer shell access. Use git to clone, fetch and push.\n");
        finish(s, channel, &msg)
    }

    async fn exec_request(&mut self, channel: ChannelId, data: &[u8], s: &mut Session) -> Result<(), Self::Error> {
        if self.channel != Some(channel) || self.execed {
            s.channel_failure(channel)?;
            return Ok(());
        }
        self.execed = true;
        self.started.store(true, Ordering::Relaxed);
        s.channel_success(channel)?;
        // Looked up again: a key removed, or an account suspended, since sign-in stops here.
        let user = match &self.key {
            Some(k) => key_owner(&self.st, k, false).await.map_err(|e| anyhow::anyhow!(e.msg))?,
            None => None,
        };
        let (Some(user), Some((service, owner, name))) = (user, parse_command(data)) else {
            self.done.notify_one();
            return finish(s, channel, "ERROR: Gitter runs only git-upload-pack, git-receive-pack and git-upload-archive on owner/repository.git, for a registered key.\n");
        };
        let (tx, rx) = mpsc::channel(64);
        self.stdin = Some(tx);
        let job = Job {
            st: self.st.clone(),
            user,
            ip: self.ip.clone(),
            service,
            owner,
            name,
            protocol: self.protocol.clone(),
            handle: s.handle(),
            channel,
            input: rx,
        };
        /// Counts the command as ended even if it panics.
        struct Ended(Arc<AtomicUsize>, Arc<tokio::sync::Notify>);
        impl Drop for Ended {
            fn drop(&mut self) {
                self.0.fetch_sub(1, Ordering::Relaxed);
                self.1.notify_one();
            }
        }
        self.running.fetch_add(1, Ordering::Relaxed);
        let ended = Ended(self.running.clone(), self.done.clone());
        tokio::spawn(async move {
            let _ended = ended;
            job.run().await;
        });
        Ok(())
    }

    async fn data(&mut self, channel: ChannelId, data: &[u8], _s: &mut Session) -> Result<(), Self::Error> {
        if self.channel == Some(channel)
            && let Some(tx) = &self.stdin
            && tx.send(data.to_vec()).await.is_err()
        {
            // The command finished or stopped reading: drop the rest.
            self.stdin = None;
        }
        Ok(())
    }

    async fn channel_eof(&mut self, _channel: ChannelId, _s: &mut Session) -> Result<(), Self::Error> {
        self.stdin = None;
        Ok(())
    }

    async fn channel_close(&mut self, channel: ChannelId, _s: &mut Session) -> Result<(), Self::Error> {
        self.stdin = None;
        if self.channel == Some(channel) {
            self.channel = None;
        }
        Ok(())
    }
}

/// Say `msg` on stderr and end the channel with exit status 1.
fn finish(s: &mut Session, channel: ChannelId, msg: &str) -> Result<(), anyhow::Error> {
    s.extended_data(channel, 1, msg.as_bytes().to_vec())?;
    s.exit_status_request(channel, 1)?;
    s.eof(channel)?;
    s.close(channel)?;
    Ok(())
}

/// What git sends: `git-upload-pack '/owner/repo.git'` (or `'owner/repo.git'`
/// for `git@host:owner/repo.git`). Returns (git subcommand, owner, repository).
fn parse_command(cmd: &[u8]) -> Option<(&'static str, String, String)> {
    let cmd = std::str::from_utf8(cmd).ok()?;
    let (verb, arg) = cmd.split_once(' ')?;
    let service = match verb {
        "git-upload-pack" => "upload-pack",
        "git-receive-pack" => "receive-pack",
        "git-upload-archive" => "upload-archive",
        _ => return None,
    };
    let path = match arg.strip_prefix('\'') {
        Some(q) => q.strip_suffix('\'')?,
        None => arg,
    };
    let path = path.strip_prefix('/').unwrap_or(path).trim_end_matches('/');
    let (owner, repo) = path.split_once('/')?;
    let bare = repo.strip_suffix(".git").unwrap_or(repo);
    if !crate::validate::name(owner, 39) || !crate::validate::name(bare, 100) {
        return None;
    }
    Some((service, owner.to_string(), bare.to_string()))
}

/// One git command on one channel.
struct Job {
    st: AppState,
    user: User,
    ip: String,
    service: &'static str,
    owner: String,
    name: String,
    protocol: Option<String>,
    handle: Handle,
    channel: ChannelId,
    input: mpsc::Receiver<Vec<u8>>,
}

impl Job {
    async fn run(self) {
        let (handle, channel) = (self.handle.clone(), self.channel);
        if let Err(e) = self.serve().await {
            let _ = handle.extended_data(channel, 1, format!("ERROR: {}\n", e.msg).into_bytes()).await;
            let _ = handle.exit_status_request(channel, 1).await;
            let _ = handle.eof(channel).await;
            let _ = handle.close(channel).await;
        }
    }

    async fn serve(self) -> R<()> {
        let Job { st, user, ip, service, owner, name, protocol, handle, channel, mut input } = self;
        // The same per-address limits as git over HTTP: starts per minute and transfers at once.
        let _slot = http::start(&st, &ip)?;
        let push = service == "receive-pack";
        let repo = http::open(&st, Some(&user), &owner, &name, push).await?;
        if service == "upload-pack" {
            st.traffic.record(repo.id, "clone", &ip, None, None);
        }
        let before = if push { Some(http::before_push(&st, &repo, Some(&user)).await?) } else { None };
        let _permit = st.git.permit_stream().await?;
        let max_push = http::push_limit(&st, &repo, push).await?;
        let (mut cmd, guard) = http::transfer_command(&st, &repo, Some(&user), max_push, push).await?;
        cmd.arg(service);
        if service == "upload-pack" {
            cmd.arg("--strict");
        }
        cmd.arg(&repo.path).stdin(Stdio::piped());
        if let Some(p) = &protocol {
            cmd.env("GIT_PROTOCOL", p);
        }
        let mut child = cmd.spawn().map_err(AppError::internal)?;
        let (mut stdin, stdout, stderr) = (child.stdin.take().expect("piped"), child.stdout.take().expect("piped"), child.stderr.take().expect("piped"));

        let activity = Arc::new(AtomicI64::new(now()));
        let cap = if push { max_push + 1024 * 1024 } else { FETCH_INPUT_CAP };
        let seen = activity.clone();
        // git refuses an oversized pack itself (receive.maxInputSize); this stops the rest of the input.
        let feeder = tokio::spawn(async move {
            let mut total: u64 = 0;
            while let Some(chunk) = input.recv().await {
                seen.store(now(), Ordering::Relaxed);
                total += chunk.len() as u64;
                if total > cap || stdin.write_all(&chunk).await.is_err() {
                    return;
                }
            }
            let _ = stdin.shutdown().await;
        });
        let outputs = async {
            let (a, b) = tokio::join!(pump(stdout, &handle, channel, None, &activity), pump(stderr, &handle, channel, Some(1), &activity));
            a && b
        };
        let idle = async {
            loop {
                tokio::time::sleep(Duration::from_secs(5)).await;
                if now() - activity.load(Ordering::Relaxed) > IDLE_SECS {
                    break;
                }
            }
        };
        // Stalled either way, or the client went away: stop git.
        let delivered = tokio::select! {
            ok = outputs => ok,
            _ = idle => false,
        };
        if !delivered {
            let _ = child.start_kill();
        }
        feeder.abort();
        let code = match tokio::time::timeout(Duration::from_secs(30), child.wait()).await {
            Ok(Ok(s)) => s.code().unwrap_or(1),
            _ => 1,
        };
        // What push protection refused is on record before the client hears the push is over.
        if let Some(g) = guard {
            g.finish(&st).await;
        }
        let _ = handle.exit_status_request(channel, code as u32).await;
        let _ = handle.eof(channel).await;
        let _ = handle.close(channel).await;
        drop(_permit);
        if let Some(before) = before
            && let Err(e) = http::after_push(&st, &repo, &user, before).await
        {
            eprintln!("post-push for {}: {}", repo.full_name(), e.msg);
        }
        Ok(())
    }
}

/// Copy git's output to the channel (`ext` 1 is stderr). False when the client stopped taking it.
async fn pump(mut r: impl AsyncRead + Unpin, handle: &Handle, channel: ChannelId, ext: Option<u32>, activity: &AtomicI64) -> bool {
    let mut buf = vec![0u8; 32 * 1024];
    loop {
        let n = match r.read(&mut buf).await {
            Ok(0) | Err(_) => return true,
            Ok(n) => n,
        };
        activity.store(now(), Ordering::Relaxed);
        let chunk = buf[..n].to_vec();
        let sent = match ext {
            None => handle.data(channel, chunk).await.is_ok(),
            Some(e) => handle.extended_data(channel, e, chunk).await.is_ok(),
        };
        if !sent {
            return false;
        }
        activity.store(now(), Ordering::Relaxed);
    }
}

// ---------- authentication keys (Settings → SSH keys) ----------

/// A pasted public key, checked: (fingerprint, `type base64`, comment).
/// Ed25519, ECDSA and RSA of 2048 bits or more, one per line, no options.
pub fn parse_key(text: &str) -> R<(String, String, String)> {
    let text = text.trim();
    if text.len() > crate::signing::MAX_KEY {
        return Err(AppError::bad("That key is too long (16 KB at most)"));
    }
    if text.contains("PRIVATE KEY") {
        return Err(AppError::bad("That's a private key. Paste the public key (the .pub file) instead, and keep the private one to yourself."));
    }
    let bad = || AppError::bad("That isn't an SSH public key: paste one line starting ssh-ed25519, ecdsa-sha2-… or ssh-rsa (the .pub file)");
    if text.lines().count() != 1 {
        return Err(bad());
    }
    let key = PublicKey::from_openssh(text).map_err(|_| bad())?;
    let data = key.key_data();
    match data.rsa() {
        Some(rsa) if rsa.key_size() < 2048 => return Err(AppError::bad("That RSA key is too short: use 2048 bits or more, or better an Ed25519 key")),
        Some(_) => {}
        None if data.is_ed25519() || data.is_ecdsa() => {}
        None => return Err(AppError::bad("Use an Ed25519, ECDSA or RSA key")),
    }
    let line = PublicKey::new(data.clone(), "").to_openssh().map_err(|_| bad())?;
    Ok((key.fingerprint(HashAlg::Sha256).to_string(), line.trim().to_string(), key.comment().to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn commands_name_one_repository() {
        let p = |s: &str| parse_command(s.as_bytes());
        assert_eq!(p("git-upload-pack '/alice/pub.git'"), Some(("upload-pack", "alice".into(), "pub".into())));
        assert_eq!(p("git-receive-pack 'alice/pub'"), Some(("receive-pack", "alice".into(), "pub".into())));
        assert_eq!(p("git-upload-archive alice/pub.git"), Some(("upload-archive", "alice".into(), "pub".into())));
        for bad in [
            "git-upload-pack '../../etc'",
            "git-upload-pack 'alice/pub.git'; ls",
            "git-upload-pack 'alice/pub.git' --help",
            "git-upload-pack '/alice/a/b'",
            "git-upload-pack '-alice/pub'",
            "git-upload-pack 'alice/..'",
            "git upload-pack 'alice/pub'",
            "git-upload-pack ",
            "ls",
            "",
        ] {
            assert_eq!(p(bad), None, "{bad}");
        }
    }

    #[test]
    fn clone_urls() {
        assert_eq!(base_url("https://git.example.com", 22), "git@git.example.com:");
        assert_eq!(base_url("http://localhost:3000/", 2222), "ssh://git@localhost:2222/");
        assert_eq!(base_url("http://[::1]:3000", 2222), "ssh://git@[::1]:2222/");
    }

    #[test]
    fn keys_are_checked() {
        let (fp, line, comment) = parse_key("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFxsPyhOovC3igNqTo8u07KB2pQB9asL+9wpOgr9jf9G my laptop").unwrap();
        assert_eq!(fp, "SHA256:tOn8xeW71HpDpSvWJO1qQFsmVDC03PosHw9pOnPgg1M", "the fingerprint ssh-keygen -l shows");
        assert_eq!(line, "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFxsPyhOovC3igNqTo8u07KB2pQB9asL+9wpOgr9jf9G");
        assert_eq!(comment, "my laptop");
        assert!(parse_key("ssh-rsa AAAAC3NzaC1lZDI1NTE5AAAAIFxsPyhOovC3igNqTo8u07KB2pQB9asL+9wpOgr9jf9G").is_err());
        assert!(parse_key("ssh-ed25519 AAAA").is_err());
    }
}
