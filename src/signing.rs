//! Commit signatures. SSH ones are checked with `ssh-keygen`, GPG ones with `gpg`
//! against a keyring of the registered keys (GITTER_DATA/gpg). The tools only say
//! whether a signature is good and which key made it; the database says whose key
//! that is, so a key removed from an account verifies nothing even while the
//! keyring still holds it.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use base64::Engine;
use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use sha2::Digest;
use tokio::io::AsyncWriteExt;

use crate::AppState;
use crate::auth::User;
use crate::error::{AppError, R};
use crate::git::{Commit, Git};
use crate::repo::Repo;

/// The longest public key accepted, in bytes.
pub const MAX_KEY: usize = 16 * 1024;
/// Signatures longer than this aren't worth running a tool on.
const MAX_SIG: usize = 64 * 1024;
/// New signature checks per request; the rest are checked on later views.
const MAX_CHECKS: usize = 100;
const TOOL_TIMEOUT: Duration = Duration::from_secs(10);
/// Commits bigger than this (a huge message) aren't read for badges.
const MAX_COMMIT: usize = 1 << 20;
const SSH_TYPES: &[&str] = &[
    "ssh-ed25519",
    "ecdsa-sha2-nistp256",
    "ecdsa-sha2-nistp384",
    "ecdsa-sha2-nistp521",
    "ssh-rsa",
    "sk-ssh-ed25519@openssh.com",
    "sk-ecdsa-sha2-nistp256@openssh.com",
];

pub struct Signing {
    /// gpg's home, holding the registered GPG keys. Signature files for the tools go here too.
    home: PathBuf,
    /// Bumped when a GPG key is added (or re-added with a new expiry or a revocation).
    /// Memoised GPG checks carry it, so ones made with the older keyring get redone.
    version: AtomicU64,
    /// The keyring is refilled from the database once per process: backups don't carry it.
    synced: tokio::sync::OnceCell<()>,
    /// How long a tool may take over one signature or key.
    timeout: Duration,
}

/// What the API says about a commit's signature.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Verification {
    /// verified, unverified, unsigned, or unknown (this server can't check it).
    pub status: String,
    /// Why, in plain words.
    pub reason: String,
    pub key_fingerprint: Option<String>,
    /// The account the signing key belongs to.
    pub signer: Option<String>,
    /// The signer proved they hold the key (signed a challenge with it when adding it);
    /// false for keys added before that was asked.
    #[serde(default)]
    pub proved: bool,
}

/// One commit's signature check. It depends only on the commit and, for GPG, the
/// keyring, so it's memoised (who owns the key is looked up fresh each time).
#[derive(Serialize, Deserialize, Clone)]
struct Check {
    outcome: Outcome,
    /// ssh or gpg.
    kind: String,
    fingerprint: Option<String>,
    committer_email: String,
    /// The keyring version a GPG check was made with; it's stale once that changes.
    keyring: u64,
    /// When the key behind a Good GPG check expires (the primary key or the subkey that
    /// signed, whichever is first): past it the check is redone, and gpg says expired.
    #[serde(default)]
    expires: Option<i64>,
    /// When it was checked.
    #[serde(default)]
    at: i64,
}

/// Is a memoised check still the answer at `now`, with keyring version `ver`?
fn fresh(c: &Check, ver: u64, now: i64) -> bool {
    if c.kind == "gpg" && c.keyring != ver {
        return false;
    }
    match c.outcome {
        Outcome::Good => c.expires.is_none_or(|e| now < e),
        Outcome::TimedOut => now < c.at + TIMEOUT_MEMO,
        _ => true,
    }
}

/// Seconds a tool's timeout is remembered: a slow signature isn't retried on every view.
const TIMEOUT_MEMO: i64 = 60;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
enum Outcome {
    Unsigned,
    Good,
    Bad,
    NoKey,
    Expired,
    Revoked,
    Unsupported,
    /// The tool took too long: no verdict, tried again after TIMEOUT_MEMO.
    TimedOut,
}

/// Why a tool gave no answer.
#[derive(Debug)]
enum Fail {
    /// Not installed (or too old to know the command).
    Missing,
    /// Failed this time.
    Broken,
    /// Ran out of time.
    TimedOut,
}

/// A public key ready to register.
pub struct NewKey {
    pub kind: &'static str,
    pub fingerprint: String,
    /// The key's comment (SSH) or user id (GPG), the default title.
    pub label: String,
    pub key: String,
}

impl Signing {
    pub fn new(home: PathBuf) -> Signing {
        Signing::with_timeout(home, TOOL_TIMEOUT)
    }

    /// With tools given `timeout` each (tests use a tiny one).
    pub fn with_timeout(home: PathBuf, timeout: Duration) -> Signing {
        Signing { home, version: AtomicU64::new(0), synced: Default::default(), timeout }
    }

    /// Call after adding a GPG key, so commits checked without it get checked again.
    /// (Removing a key needs nothing: who owns a key is looked up on every request.)
    pub fn key_added(&self) {
        self.version.fetch_add(1, Ordering::SeqCst);
    }

    fn ensure_home(&self) -> Result<(), Fail> {
        let mut b = std::fs::DirBuilder::new();
        b.recursive(true);
        // gpg wants its home private.
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut b, 0o700);
        b.create(&self.home).map_err(|_| Fail::Broken)
    }

    /// Run `prog` with `input` on stdin: scrubbed environment, a timeout, and git's process cap.
    async fn tool(&self, git: &Git, prog: &str, args: &[&str], input: &[u8], timeout: Duration) -> Result<std::process::Output, Fail> {
        let _permit = git.permit().await.map_err(|_| Fail::Broken)?;
        let mut child = tokio::process::Command::new(prog)
            .env_clear()
            .env("PATH", std::env::var_os("PATH").unwrap_or_else(|| "/usr/bin:/bin".into()))
            .env("HOME", &self.home)
            .env("LANG", "C")
            .args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| if e.kind() == std::io::ErrorKind::NotFound { Fail::Missing } else { Fail::Broken })?;
        let mut stdin = child.stdin.take().expect("piped stdin");
        let data = input.to_vec();
        tokio::spawn(async move {
            let _ = stdin.write_all(&data).await;
        });
        match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(out)) => Ok(out),
            Ok(Err(_)) => Err(Fail::Broken),
            Err(_) => Err(Fail::TimedOut),
        }
    }

    /// Import armored GPG keys into the keyring at `home`; returns gpg's status lines.
    async fn gpg_import(&self, git: &Git, home: &Path, keys: &[u8], timeout: Duration) -> Result<String, Fail> {
        self.ensure_home()?;
        let h = home.to_string_lossy();
        let args = ["--homedir", &h, "--batch", "--no-tty", "--no-autostart", "--status-fd", "1", "--import"];
        let o = self.tool(git, "gpg", &args, keys, timeout).await?;
        Ok(String::from_utf8_lossy(&o.stdout).into_owned())
    }

    /// A GPG key now registered to an account: into the server's keyring, and
    /// commits checked without it get checked again.
    pub async fn add_gpg_key(&self, git: &Git, key: &str) {
        if let Err(f) = self.gpg_import(git, &self.home, key.as_bytes(), self.timeout).await {
            eprintln!("signing: importing a GPG key failed ({f:?}); it's imported at the next start");
        }
        self.key_added();
    }

    /// `sig` in a file of its own in the home, for a tool to read; removed when dropped.
    async fn sig_file(&self, sig: &str) -> Result<TempFile, Fail> {
        self.ensure_home()?;
        let file = TempFile(self.home.join(format!("sig-{}", random_hex())));
        tokio::fs::write(&file.0, sig).await.map_err(|_| Fail::Broken)?;
        Ok(file)
    }

    /// An empty, private gpg home of its own; removed when dropped.
    fn scratch(&self) -> Result<TempDir, Fail> {
        self.ensure_home()?;
        let dir = TempDir(self.home.join(format!("parse-{}", random_hex())));
        let mut b = std::fs::DirBuilder::new();
        #[cfg(unix)]
        std::os::unix::fs::DirBuilderExt::mode(&mut b, 0o700);
        b.create(&dir.0).map_err(|_| Fail::Broken)?;
        Ok(dir)
    }

    /// `ssh-keygen -Y check-novalidate`: is `sig` good over `payload` in `namespace`, and by which key?
    async fn ssh_verify(&self, git: &Git, namespace: &str, payload: &[u8], sig: &str) -> Result<(Outcome, Option<String>), Fail> {
        let file = self.sig_file(sig).await?;
        let f = file.0.to_string_lossy();
        let args = ["-Y", "check-novalidate", "-n", namespace, "-s", &f];
        self.tool(git, "ssh-keygen", &args, payload, self.timeout).await.and_then(|o| parse_ssh(&o, namespace))
    }

    /// gpg's status lines for detached signature `sig` over `payload`, against the keyring at `home`.
    async fn gpg_verify(&self, git: &Git, home: &Path, payload: &[u8], sig: &str) -> Result<String, Fail> {
        let file = self.sig_file(sig).await?;
        let (f, h) = (file.0.to_string_lossy(), home.to_string_lossy());
        let args = [
            "--homedir", &h, "--batch", "--no-tty", "--no-autostart", "--no-auto-key-retrieve", "--trust-model", "always", "--status-fd", "1",
            "--verify", &f, "-",
        ];
        let o = self.tool(git, "gpg", &args, payload, self.timeout).await?;
        let status = String::from_utf8_lossy(&o.stdout).into_owned();
        // No verdict at all and a failed exit: gpg itself broke (a locked keyring...). Not cached.
        if !o.status.success() && !status.lines().any(|l| SIG_STATUS.iter().any(|k| l.starts_with(k))) {
            return Err(Fail::Broken);
        }
        Ok(status)
    }

    /// Put every registered GPG key in the keyring, once per process.
    async fn sync(&self, st: &AppState) -> Result<(), Fail> {
        self.synced
            .get_or_try_init(|| async {
                let keys: Vec<String> = st
                    .db
                    .call(|c| {
                        let mut s = c.prepare("SELECT key FROM signing_keys WHERE kind = 'gpg'")?;
                        let v = s.query_map([], |r| r.get(0))?.collect::<Result<Vec<String>, _>>()?;
                        Ok(v)
                    })
                    .await
                    .map_err(|_| Fail::Broken)?;
                // ponytail: one import of every key; a few minutes allowed. Chunk it if instances get huge.
                // Leftovers of checks and key parsing a crash cut short.
                if let Ok(rd) = std::fs::read_dir(&self.home) {
                    for e in rd.flatten() {
                        let n = e.file_name().to_string_lossy().into_owned();
                        if n.starts_with("sig-") || n.starts_with("parse-") {
                            let _ = std::fs::remove_dir_all(e.path()).or_else(|_| std::fs::remove_file(e.path()));
                        }
                    }
                }
                if !keys.is_empty() {
                    self.gpg_import(&st.git, &self.home, keys.join("\n").as_bytes(), Duration::from_secs(300)).await?;
                }
                Ok(())
            })
            .await
            .map(|_| ())
    }

    /// Check one signature over `payload`. Err is the reason it couldn't be checked.
    async fn check(&self, st: &AppState, payload: &[u8], sig: &str) -> Result<(Outcome, &'static str, Option<String>, Option<i64>), &'static str> {
        let kind = if sig.starts_with("-----BEGIN SSH SIGNATURE-----") {
            "ssh"
        } else if sig.starts_with("-----BEGIN PGP SIGNATURE-----") {
            "gpg"
        } else {
            return Ok((Outcome::Unsupported, "", None, None));
        };
        if sig.len() > MAX_SIG {
            return Ok((Outcome::Unsupported, kind, None, None));
        }
        match self.run_check(st, kind, payload, sig).await {
            Ok((o, fp, exp)) => Ok((o, kind, fp, exp)),
            // Remembered for a minute, so a signature that hangs the tool isn't run on every view.
            Err(Fail::TimedOut) => Ok((Outcome::TimedOut, kind, None, None)),
            Err(Fail::Missing) if kind == "ssh" => Err("This server can't check SSH signatures (it needs ssh-keygen from OpenSSH 8.4 or newer)."),
            Err(Fail::Missing) => Err("This server can't check GPG signatures (gpg isn't installed)."),
            Err(Fail::Broken) => Err("This signature couldn't be checked just now. Reload the page to try again."),
        }
    }

    /// The outcome, the key's fingerprint and, for a good GPG signature, when the key expires.
    async fn run_check(&self, st: &AppState, kind: &str, payload: &[u8], sig: &str) -> Result<(Outcome, Option<String>, Option<i64>), Fail> {
        if kind == "ssh" {
            // Checks the signature against the public key inside it; whose key that is, the database says.
            return self.ssh_verify(&st.git, "git", payload, sig).await.map(|(o, fp)| (o, fp, None));
        }
        // Only signature packets: gpg would otherwise check a signed message carried
        // in the header (and ignore the commit) and still report a good signature.
        if !only_signature_packets(sig) {
            return Ok((Outcome::Bad, None, None));
        }
        self.sync(st).await?;
        let status = self.gpg_verify(&st.git, &self.home, payload, sig).await?;
        let (outcome, fpr) = parse_gpg(&status);
        let (Outcome::Good, Some(primary), Some(sub)) = (outcome, &fpr, signing_subkey(&status)) else { return Ok((outcome, fpr, None)) };
        // ponytail: one more gpg run per good GPG check (they're memoised); memo per key if it shows.
        let h = self.home.to_string_lossy();
        let args = ["--homedir", &h, "--batch", "--no-tty", "--no-autostart", "--with-colons", "--fixed-list-mode", "--list-keys", primary.as_str()];
        let o = self.tool(&st.git, "gpg", &args, b"", self.timeout).await?;
        if !o.status.success() {
            return Err(Fail::Broken);
        }
        let expires = key_expiry(&String::from_utf8_lossy(&o.stdout), primary, sub);
        // Expired since gpg answered: say so rather than cache a Good that's already over.
        if expires.is_some_and(|e| e <= crate::db::now()) {
            return Ok((Outcome::Expired, fpr, None));
        }
        Ok((outcome, fpr, expires))
    }
}

/// Removed when dropped, even when a request is abandoned mid-check.
struct TempFile(PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn random_hex() -> String {
    let mut b = [0u8; 8];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut b);
    hex::encode(b)
}

/// `ssh-keygen -Y check-novalidate` output: `Good "git" signature with ED25519 key SHA256:…`.
fn parse_ssh(o: &std::process::Output, namespace: &str) -> Result<(Outcome, Option<String>), Fail> {
    // OpenSSH before 8.4 has no check-novalidate and prints its usage.
    if String::from_utf8_lossy(&o.stderr).contains("usage:") {
        return Err(Fail::Missing);
    }
    let out = String::from_utf8_lossy(&o.stdout);
    let fp = out
        .lines()
        .next()
        .filter(|l| o.status.success() && l.starts_with(&format!("Good \"{namespace}\" signature")))
        .and_then(|l| l.rsplit_once(" key "))
        .map(|(_, f)| f.trim().to_string())
        .filter(|f| f.starts_with("SHA256:"));
    Ok(match fp {
        Some(f) => (Outcome::Good, Some(f)),
        None => (Outcome::Bad, None),
    })
}

/// Status lines that mean gpg reached a verdict about a signature.
const SIG_STATUS: &[&str] = &[
    "[GNUPG:] NEWSIG", "[GNUPG:] GOODSIG", "[GNUPG:] BADSIG", "[GNUPG:] ERRSIG", "[GNUPG:] VALIDSIG",
    "[GNUPG:] EXPSIG", "[GNUPG:] EXPKEYSIG", "[GNUPG:] REVKEYSIG", "[GNUPG:] NODATA",
];

/// Is this armored block nothing but OpenPGP signature packets (tag 2), as a
/// detached signature is? A signed message (one-pass signature, literal data,
/// compressed data) is refused.
fn only_signature_packets(armored: &str) -> bool {
    let mut lines = armored.lines().map(str::trim).skip_while(|l| !l.starts_with("-----BEGIN PGP SIGNATURE-----")).skip(1);
    // Armor headers ("Version: ..."), then a blank line, then base64 up to the "=CRC" line.
    let mut body = String::new();
    let mut in_headers = true;
    for l in lines.by_ref() {
        if l.starts_with("-----END") {
            break;
        }
        if in_headers {
            if l.is_empty() {
                in_headers = false;
            } else if !l.contains(": ") {
                in_headers = false;
                body.push_str(l);
            }
            continue;
        }
        if l.starts_with('=') {
            continue;
        }
        body.push_str(l);
    }
    let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(body) else { return false };
    let mut rest = &bytes[..];
    let mut packets = 0;
    while let Some((&b, tail)) = rest.split_first() {
        if b & 0x80 == 0 {
            return false;
        }
        let (tag, len, tail) = if b & 0x40 != 0 {
            // New format: one, two or five length octets (partial lengths aren't used by signatures).
            match tail {
                [o1, t @ ..] if *o1 < 192 => (b & 0x3f, *o1 as usize, t),
                [o1, o2, t @ ..] if *o1 < 224 => (b & 0x3f, ((*o1 as usize - 192) << 8) + *o2 as usize + 192, t),
                [255, a, b2, c, d, t @ ..] => (b & 0x3f, u32::from_be_bytes([*a, *b2, *c, *d]) as usize, t),
                _ => return false,
            }
        } else {
            let tag = (b >> 2) & 0x0f;
            match (b & 3, tail) {
                (0, [l, t @ ..]) => (tag, *l as usize, t),
                (1, [a, b2, t @ ..]) => (tag, u16::from_be_bytes([*a, *b2]) as usize, t),
                (2, [a, b2, c, d, t @ ..]) => (tag, u32::from_be_bytes([*a, *b2, *c, *d]) as usize, t),
                _ => return false,
            }
        };
        if tag != 2 || len > tail.len() {
            return false;
        }
        rest = &tail[len..];
        packets += 1;
    }
    packets > 0
}

/// gpg's machine-readable status lines (`--status-fd`).
fn parse_gpg(status: &str) -> (Outcome, Option<String>) {
    let (mut good, mut bad, mut expired, mut revoked, mut nokey, mut errsig) = (false, false, false, false, false, false);
    let mut fpr = None;
    for line in status.lines() {
        let Some(rest) = line.strip_prefix("[GNUPG:] ") else { continue };
        let t: Vec<&str> = rest.split(' ').collect();
        match t[0] {
            "GOODSIG" => good = true,
            // Literal data means a signed message, not a detached signature over the commit.
            "BADSIG" | "PLAINTEXT" => bad = true,
            "EXPSIG" | "EXPKEYSIG" => expired = true,
            "REVKEYSIG" => revoked = true,
            // The primary key's fingerprint comes last: keys are registered by it.
            "VALIDSIG" => fpr = t.get(10).or(t.get(1)).map(|s| s.to_string()),
            // Return code 9 is a missing public key; the signing key's fingerprint follows if known.
            "ERRSIG" => {
                errsig = true;
                nokey |= t.get(6) == Some(&"9");
                if fpr.is_none() {
                    fpr = t.get(7).filter(|f| **f != "-").map(|s| s.to_string());
                }
            }
            _ => {}
        }
    }
    let o = if bad {
        Outcome::Bad
    } else if revoked {
        Outcome::Revoked
    } else if expired {
        Outcome::Expired
    } else if good && fpr.is_some() {
        Outcome::Good
    } else if nokey {
        Outcome::NoKey
    } else if errsig {
        Outcome::Unsupported
    } else {
        Outcome::Bad
    };
    (o, fpr)
}

/// The fingerprint of the (sub)key that made a good GPG signature: VALIDSIG's first field.
fn signing_subkey(status: &str) -> Option<&str> {
    status.lines().find_map(|l| l.strip_prefix("[GNUPG:] VALIDSIG ")).and_then(|r| r.split(' ').next())
}

/// When a key stops being good for signatures by `sub` (the primary key or one of its
/// subkeys), from `gpg --with-colons --list-keys`: the earlier of the two expiries.
fn key_expiry(colons: &str, primary: &str, sub: &str) -> Option<i64> {
    let mut last: Option<i64> = None;
    let mut soonest: Option<i64> = None;
    for l in colons.lines() {
        let f: Vec<&str> = l.split(':').collect();
        match f[0] {
            // Field 7 is the expiry (seconds since 1970), empty for never.
            "pub" | "sub" => last = f.get(6).and_then(|e| e.parse().ok()),
            "fpr" if f.get(9).is_some_and(|x| *x == primary || *x == sub) => {
                if let Some(e) = last.take() {
                    soonest = Some(soonest.map_or(e, |s| s.min(e)));
                }
            }
            "fpr" => last = None,
            _ => {}
        }
    }
    soonest
}

/// Check a pasted public key. A GPG key is read in a throwaway keyring; it joins
/// the server's keyring only once it's registered (`add_gpg_key`).
pub async fn parse_key(st: &AppState, text: &str) -> R<NewKey> {
    let text = text.trim();
    if text.len() > MAX_KEY {
        return Err(AppError::bad("That key is too long (16 KB at most)"));
    }
    if text.contains("PRIVATE KEY") {
        return Err(AppError::bad("That's a private key. Paste the public key instead, and keep the private one to yourself."));
    }
    if text.starts_with("-----BEGIN PGP PUBLIC KEY BLOCK-----") {
        return gpg_key(st, text).await;
    }
    ssh_key(text).ok_or_else(|| {
        AppError::bad("That isn't an SSH public key (a line starting ssh-ed25519, ecdsa-sha2-… or ssh-rsa) or a GPG public key block")
    })
}

fn ssh_key(text: &str) -> Option<NewKey> {
    if text.lines().count() != 1 {
        return None;
    }
    let mut parts = text.split_whitespace();
    let (kind, b64) = (parts.next()?, parts.next()?);
    if !SSH_TYPES.contains(&kind) {
        return None;
    }
    let blob = base64::engine::general_purpose::STANDARD.decode(b64).ok()?;
    // The key names its own type: a mismatch means it's mangled or mislabelled.
    let n = u32::from_be_bytes(blob.get(..4)?.try_into().ok()?) as usize;
    if blob.get(4..4usize.checked_add(n)?)? != kind.as_bytes() {
        return None;
    }
    let fp = base64::engine::general_purpose::STANDARD_NO_PAD.encode(sha2::Sha256::digest(&blob));
    Some(NewKey {
        kind: "ssh",
        fingerprint: format!("SHA256:{fp}"),
        label: parts.collect::<Vec<_>>().join(" "),
        key: format!("{kind} {b64}"),
    })
}

async fn gpg_key(st: &AppState, text: &str) -> R<NewKey> {
    let scratch = st.signing.scratch().map_err(|_| AppError::internal("can't make a scratch GPG home"))?;
    let status = st.signing.gpg_import(&st.git, &scratch.0, text.as_bytes(), st.signing.timeout).await.map_err(|f| match f {
        Fail::Missing => AppError::bad("This server can't read GPG keys (gpg isn't installed)"),
        Fail::Broken | Fail::TimedOut => AppError::new(axum::http::StatusCode::SERVICE_UNAVAILABLE, "Couldn't read the key just now. Try again."),
    })?;
    let fprs: HashSet<&str> = status
        .lines()
        .filter_map(|l| l.strip_prefix("[GNUPG:] IMPORT_OK "))
        .filter_map(|r| r.split(' ').nth(1))
        .collect();
    let label = status
        .lines()
        .find_map(|l| l.strip_prefix("[GNUPG:] IMPORTED "))
        .and_then(|r| r.split_once(' '))
        .map(|(_, uid)| uid.to_string())
        .unwrap_or_default();
    match fprs.into_iter().collect::<Vec<_>>()[..] {
        [f] => Ok(NewKey { kind: "gpg", fingerprint: f.to_string(), label, key: text.to_string() }),
        [] => Err(AppError::bad("That isn't a GPG public key gpg can read")),
        _ => Err(AppError::bad("Add one GPG key at a time")),
    }
}

/// SSH signatures proving a key are made in this namespace, so a commit signature can't stand in.
pub const PROOF_NAMESPACE: &str = "gitter";

/// Check that `sig` is key `k`'s signature over `challenge`: whoever adds a key must hold
/// its private half. A GPG signature is checked in a keyring holding only that key.
pub async fn prove(st: &AppState, k: &NewKey, challenge: &str, sig: &str) -> R<()> {
    let s = &st.signing;
    let sig = sig.trim().replace("\r\n", "\n") + "\n";
    let wrong = || AppError::bad("That isn't this key's signature over the challenge. Sign the exact text shown, with the key you're adding.");
    let armor = if k.kind == "ssh" { "-----BEGIN SSH SIGNATURE-----" } else { "-----BEGIN PGP SIGNATURE-----" };
    if sig.len() > MAX_SIG || !sig.starts_with(armor) || (k.kind == "gpg" && !only_signature_packets(&sig)) {
        return Err(wrong());
    }
    let res = if k.kind == "ssh" {
        s.ssh_verify(&st.git, PROOF_NAMESPACE, challenge.as_bytes(), &sig).await
    } else {
        async {
            let scratch = s.scratch()?;
            s.gpg_import(&st.git, &scratch.0, k.key.as_bytes(), s.timeout).await?;
            Ok(parse_gpg(&s.gpg_verify(&st.git, &scratch.0, challenge.as_bytes(), &sig).await?))
        }
        .await
    };
    match res {
        Ok((Outcome::Good, Some(fp))) if fp == k.fingerprint => Ok(()),
        Ok((Outcome::Expired, _)) => Err(AppError::bad("That key has expired. Extend it (gpg --quick-set-expire), then export and add it again.")),
        Ok((Outcome::Revoked, _)) => Err(AppError::bad("That key was revoked.")),
        Ok(_) => Err(wrong()),
        Err(Fail::Missing) => Err(AppError::bad(format!("This server can't check {} signatures.", k.kind.to_uppercase()))),
        Err(Fail::Broken | Fail::TimedOut) => Err(AppError::new(axum::http::StatusCode::SERVICE_UNAVAILABLE, "Couldn't check the signature just now. Try again.")),
    }
}

/// Fill in each commit's `verification`.
pub async fn annotate(st: &AppState, r: &Repo, commits: &mut [Commit]) {
    let shas: Vec<&str> = commits.iter().map(|c| c.sha.as_str()).collect();
    let mut found = verify(st, r, &shas).await;
    for c in commits {
        c.verification = found.remove(&c.sha);
    }
}

/// The signature status of each commit in `shas`, by sha. Badges are extras:
/// if checking fails, the page still shows, just without them.
pub async fn verify(st: &AppState, r: &Repo, shas: &[&str]) -> HashMap<String, Verification> {
    try_verify(st, r, shas).await.unwrap_or_else(|e| {
        eprintln!("signature checks in {}: {}", r.full_name(), e.msg);
        HashMap::new()
    })
}

async fn try_verify(st: &AppState, r: &Repo, shas: &[&str]) -> R<HashMap<String, Verification>> {
    let s = &st.signing;
    // Read before checking: a key added meanwhile makes these checks stale, never wrongly fresh.
    let ver = s.version.load(Ordering::SeqCst);
    let key = |sha: &str| format!("sig:{}:{sha}", r.id);
    let mut checks: HashMap<String, Result<Check, Verification>> = HashMap::new();
    let mut misses = Vec::new();
    let mut seen = HashSet::new();
    let now = crate::db::now();
    for sha in shas.iter().copied().filter(|s| seen.insert(*s)) {
        match st.git.memo.get::<Check>(&key(sha)) {
            Some(c) if fresh(&c, ver, now) => {
                checks.insert(sha.to_string(), Ok(c));
            }
            _ => misses.push(sha),
        }
    }
    let mut todo = Vec::new();
    for (sha, raw) in raw_commits(&st.git, &r.path, &misses).await? {
        let committer_email = committer_email(&raw);
        match split_signature(&raw) {
            Some((payload, sig)) => todo.push((sha, payload, sig, committer_email)),
            None => {
                let c = Check { outcome: Outcome::Unsigned, kind: String::new(), fingerprint: None, committer_email, keyring: ver, expires: None, at: now };
                st.git.memo.put(key(&sha), &c);
                checks.insert(sha, Ok(c));
            }
        }
    }
    // ponytail: at most MAX_CHECKS new checks per request, 8 at a time; the rest say
    // "not checked yet" and get their turn on a later view.
    let later = todo.split_off(todo.len().min(MAX_CHECKS));
    for (sha, ..) in later {
        checks.insert(sha, Err(unknown("This signature hasn't been checked yet. Reload the page to check it.")));
    }
    let done: Vec<_> = futures_util::stream::iter(todo)
        .map(|(sha, payload, sig, email)| async move { (sha, s.check(st, &payload, &sig).await, email) })
        .buffer_unordered(8)
        .collect()
        .await;
    for (sha, res, committer_email) in done {
        match res {
            Ok((outcome, kind, fingerprint, expires)) => {
                let c = Check { outcome, kind: kind.into(), fingerprint, committer_email, keyring: ver, expires, at: now };
                st.git.memo.put(key(&sha), &c);
                checks.insert(sha, Ok(c));
            }
            // Not memoised: the tool may be installed, or work, next time.
            Err(why) => {
                checks.insert(sha, Err(unknown(why)));
            }
        }
    }
    let good: HashSet<String> = checks
        .values()
        .filter_map(|c| c.as_ref().ok())
        .filter(|c| c.outcome == Outcome::Good)
        .filter_map(|c| c.fingerprint.clone())
        .collect();
    let owners = owners(st, good).await?;
    Ok(checks
        .into_iter()
        .map(|(sha, c)| {
            let v = match c {
                Ok(c) => verdict(&c, c.fingerprint.as_ref().and_then(|f| owners.get(f)).map_or(&[][..], Vec::as_slice)),
                Err(v) => v,
            };
            (sha, v)
        })
        .collect())
}

fn unknown(why: &str) -> Verification {
    Verification { status: "unknown".into(), reason: why.into(), key_fingerprint: None, signer: None, proved: false }
}

/// The accounts that registered each of these fingerprints, with the key's kind and
/// whether they proved they hold it.
async fn owners(st: &AppState, fprs: HashSet<String>) -> R<HashMap<String, Vec<(String, User, bool)>>> {
    if fprs.is_empty() {
        return Ok(HashMap::new());
    }
    let fprs: Vec<String> = fprs.into_iter().collect();
    st.db
        .call(move |c| {
            let marks = vec!["?"; fprs.len()].join(",");
            let mut s = c.prepare(&format!(
                "SELECT k.fingerprint, k.kind, k.proved_at IS NOT NULL, u.id, u.username, u.display_name, u.email, u.is_admin
                 FROM signing_keys k JOIN users u ON u.id = k.user_id WHERE k.fingerprint IN ({marks})"
            ))?;
            let rows = s.query_map(rusqlite::params_from_iter(&fprs), |r| {
                Ok((r.get::<_, String>(0)?, (r.get::<_, String>(1)?, User::from_row(r)?, r.get::<_, bool>(2)?)))
            })?;
            let mut out: HashMap<String, Vec<(String, User, bool)>> = HashMap::new();
            for row in rows {
                let (f, o) = row?;
                out.entry(f).or_default().push(o);
            }
            Ok(out)
        })
        .await
}

fn verdict(c: &Check, owners: &[(String, User, bool)]) -> Verification {
    let (kind, a) = if c.kind == "ssh" { ("SSH", "an") } else { ("GPG", "a") };
    let says = |status: &str, reason: String, signer: Option<(&User, bool)>| Verification {
        status: status.into(),
        reason,
        key_fingerprint: c.fingerprint.clone(),
        signer: signer.map(|(u, _)| u.username.clone()),
        proved: signer.is_some_and(|(_, p)| p),
    };
    let unregistered = || says("unverified", format!("Signed with {a} {kind} key that isn't registered to any account here."), None);
    match c.outcome {
        Outcome::Unsigned => Verification { status: "unsigned".into(), reason: String::new(), key_fingerprint: None, signer: None, proved: false },
        Outcome::Bad => says("unverified", "The signature doesn't match this commit, so the commit may have been changed after it was signed.".into(), None),
        Outcome::Expired => says("unverified", "The signature, or the GPG key that made it, has expired.".into(), None),
        Outcome::Revoked => says("unverified", "The GPG key that made this signature was revoked.".into(), None),
        Outcome::Unsupported => says("unverified", "This kind of signature can't be checked here (only SSH and GPG signatures can).".into(), None),
        Outcome::NoKey => unregistered(),
        Outcome::TimedOut => unknown("Checking this signature took too long. It's tried again a minute later."),
        Outcome::Good => {
            // Several accounts may have added the same public key: the signature
            // verifies for the one whose email is the committer's. Once someone proved they
            // hold it, those who only pasted it (before proof was asked for) don't count.
            let mut mine: Vec<(&User, bool)> = owners.iter().filter(|(k, ..)| *k == c.kind).map(|(_, u, p)| (u, *p)).collect();
            if mine.iter().any(|(_, p)| *p) {
                mine.retain(|(_, p)| *p);
            }
            match (mine.iter().find(|(u, _)| owns_email(u, &c.committer_email)), mine.as_slice()) {
                (Some(&(u, proved)), _) => {
                    let name = &u.username;
                    let reason = if proved {
                        format!("Signed with {name}'s {kind} key, which {name} proved they hold.")
                    } else {
                        format!("Signed with {name}'s {kind} key. It was added before keys needed proof that the account holds them.")
                    };
                    says("verified", reason, Some((u, proved)))
                }
                (None, []) => unregistered(),
                (None, &[(u, proved)]) => {
                    let name = &u.username;
                    says("unverified", format!("Signed with {name}'s {kind} key, but the committer email isn't {name}'s."), Some((u, proved)))
                }
                (None, _) => says("unverified", format!("Signed with a registered {kind} key, but the committer email isn't that of anyone who added it."), None),
            }
        }
    }
}

/// Accounts have one email and no verified extras: the committer email must be
/// that one, or the account's no-reply address that web commits use.
fn owns_email(u: &User, email: &str) -> bool {
    let e = email.trim();
    !e.is_empty() && (e.eq_ignore_ascii_case(&u.email) || e.eq_ignore_ascii_case(&u.signature().1))
}

/// Raw commit objects for `shas`, read with one `git cat-file --batch`.
async fn raw_commits(git: &Git, repo: &Path, shas: &[&str]) -> R<Vec<(String, Vec<u8>)>> {
    let shas: Vec<&str> = shas.iter().copied().filter(|s| s.len() >= 40 && s.bytes().all(|b| b.is_ascii_hexdigit())).collect();
    if shas.is_empty() {
        return Ok(vec![]);
    }
    // Sizes first: a commit with a multi-megabyte message isn't read for a badge.
    let input = shas.join("\n") + "\n";
    let sizes = git.run(repo, &["cat-file", "--batch-check"], Some(input.as_bytes()), &[]).await?;
    let small: Vec<String> = String::from_utf8_lossy(&sizes.stdout)
        .lines()
        .filter_map(|l| match l.split(' ').collect::<Vec<_>>()[..] {
            [sha, "commit", n] if n.parse::<usize>().is_ok_and(|n| n <= MAX_COMMIT) => Some(sha.to_string()),
            _ => None,
        })
        .collect();
    if small.is_empty() {
        return Ok(vec![]);
    }
    let input = small.join("\n") + "\n";
    let o = git.run(repo, &["cat-file", "--batch"], Some(input.as_bytes()), &[]).await?;
    if o.code != 0 {
        return Err(AppError::internal(format!("git cat-file: {}", String::from_utf8_lossy(&o.stderr).trim())));
    }
    Ok(parse_batch(&o.stdout))
}

/// `cat-file --batch` output: `<sha> <type> <size>\n<content>\n` per object, or `<sha> missing\n`.
fn parse_batch(mut out: &[u8]) -> Vec<(String, Vec<u8>)> {
    let mut v = Vec::new();
    while let Some(nl) = out.iter().position(|b| *b == b'\n') {
        let head = String::from_utf8_lossy(&out[..nl]).into_owned();
        out = &out[nl + 1..];
        let f: Vec<&str> = head.split(' ').collect();
        let [sha, kind, size] = f.as_slice() else { continue };
        let Some(body) = size.parse::<usize>().ok().and_then(|n| out.get(..n)) else { break };
        if *kind == "commit" {
            v.push((sha.to_string(), body.to_vec()));
        }
        out = out.get(body.len() + 1..).unwrap_or_default();
    }
    v
}

/// The commit's headers: everything before the first blank line.
fn headers(raw: &[u8]) -> &[u8] {
    raw.windows(2).position(|w| w == b"\n\n").map_or(raw, |i| &raw[..i + 1])
}

fn committer_email(raw: &[u8]) -> String {
    headers(raw)
        .split(|b| *b == b'\n')
        .find_map(|l| l.strip_prefix(b"committer "))
        .map(String::from_utf8_lossy)
        .and_then(|l| Some(l.get(l.find('<')? + 1..l.rfind('>')?)?.to_string()))
        .unwrap_or_default()
}

/// Split a raw commit into what was signed (the commit without its `gpgsig`
/// headers, as git does) and the signature. None when it isn't signed.
fn split_signature(raw: &[u8]) -> Option<(Vec<u8>, String)> {
    let head = headers(raw);
    let mut payload = Vec::with_capacity(raw.len());
    let mut sig = String::new();
    let (mut in_sig, mut in_other) = (false, false);
    for line in head.split_inclusive(|b| *b == b'\n') {
        if line.starts_with(b" ") && (in_sig || in_other) {
            if in_sig {
                sig.push_str(&String::from_utf8_lossy(&line[1..]));
            }
            continue;
        }
        (in_sig, in_other) = (false, false);
        if let Some(v) = line.strip_prefix(b"gpgsig ") {
            in_sig = true;
            sig.push_str(&String::from_utf8_lossy(v));
        } else if line.starts_with(b"gpgsig") {
            // gpgsig-sha256 (the signature for the repo's other hash) isn't part of the payload either.
            in_other = true;
        } else {
            payload.extend_from_slice(line);
        }
    }
    payload.extend_from_slice(&raw[head.len()..]);
    (!sig.is_empty()).then_some((payload, sig))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signature_splits_off_like_git() {
        let raw = b"tree t\nauthor A <a@x> 1 +0000\ncommitter C <c@x.io> 2 +0000\ngpgsig -----BEGIN SSH SIGNATURE-----\n abc\n -----END SSH SIGNATURE-----\ngpgsig-sha256 other\n more\n\nmsg\n\ngpgsig in the body\n";
        let (payload, sig) = split_signature(raw).unwrap();
        assert_eq!(payload, b"tree t\nauthor A <a@x> 1 +0000\ncommitter C <c@x.io> 2 +0000\n\nmsg\n\ngpgsig in the body\n");
        assert_eq!(sig, "-----BEGIN SSH SIGNATURE-----\nabc\n-----END SSH SIGNATURE-----\n");
        assert_eq!(committer_email(raw), "c@x.io");
        assert!(split_signature(b"tree t\n\ngpgsig not a header\n").is_none(), "only headers count");
    }

    #[test]
    fn batch_output_parsing_skips_missing_objects() {
        let out = b"aaa commit 3\nabc\nbbb missing\nccc tree 2\nxy\nddd commit 0\n\n";
        let v = parse_batch(out);
        assert_eq!(v, vec![("aaa".to_string(), b"abc".to_vec()), ("ddd".to_string(), vec![])]);
    }

    #[test]
    fn gpg_status_lines() {
        let good = "[GNUPG:] NEWSIG\n[GNUPG:] GOODSIG 4B9A Alice\n[GNUPG:] VALIDSIG SUB 2026-10-22 1 0 4 0 22 10 00 PRIMARY\n";
        assert_eq!(parse_gpg(good), (Outcome::Good, Some("PRIMARY".into())));
        assert_eq!(parse_gpg("[GNUPG:] BADSIG 4B9A Alice\n").0, Outcome::Bad);
        let missing = "[GNUPG:] ERRSIG 4B9A 22 10 00 1 9 FPR\n[GNUPG:] NO_PUBKEY 4B9A\n";
        assert_eq!(parse_gpg(missing), (Outcome::NoKey, Some("FPR".into())));
        assert_eq!(parse_gpg("[GNUPG:] EXPKEYSIG 4B9A Alice\n[GNUPG:] VALIDSIG SUB x\n").0, Outcome::Expired);
        assert_eq!(parse_gpg("[GNUPG:] NODATA 1\n").0, Outcome::Bad);
    }

    #[test]
    fn only_detached_signatures_count() {
        use base64::Engine;
        let arm = |b: &[u8]| format!("-----BEGIN PGP SIGNATURE-----\n\n{}\n=abcd\n-----END PGP SIGNATURE-----\n", base64::engine::general_purpose::STANDARD.encode(b));
        // New-format signature packet (0xc2), then old-format (0x88): fine.
        assert!(only_signature_packets(&arm(&[0xc2, 3, 1, 2, 3, 0x88, 2, 9, 9])));
        // One-pass signature (tag 4), literal data (tag 11), compressed (tag 8): refused.
        assert!(!only_signature_packets(&arm(&[0xc4, 1, 0, 0xc2, 1, 0])));
        assert!(!only_signature_packets(&arm(&[0xc2, 1, 0, 0xcb, 1, 0])), "an old-style signed message");
        assert!(!only_signature_packets(&arm(&[0xa3, 1, 0])));
        assert!(!only_signature_packets(&arm(&[0xc2, 9, 1])), "length past the end");
        assert!(!only_signature_packets(&arm(&[])));
        assert!(only_signature_packets("-----BEGIN PGP SIGNATURE-----\nVersion: x\n\nwgMBAgM=\n=x\n-----END PGP SIGNATURE-----"));
        assert_eq!(parse_gpg("[GNUPG:] PLAINTEXT 62 0\n[GNUPG:] GOODSIG 4B9A Alice\n[GNUPG:] VALIDSIG S x\n").0, Outcome::Bad);
    }

    fn check(outcome: Outcome, kind: &str, expires: Option<i64>) -> Check {
        Check { outcome, kind: kind.into(), fingerprint: None, committer_email: String::new(), keyring: 3, expires, at: 1000 }
    }

    #[test]
    fn cached_gpg_good_results_last_until_the_key_expires() {
        let c = check(Outcome::Good, "gpg", Some(2000));
        assert!(fresh(&c, 3, 1999));
        assert!(!fresh(&c, 3, 2000), "expired: checked again (gpg then says so)");
        assert!(!fresh(&c, 4, 1500), "the keyring changed");
        assert!(fresh(&check(Outcome::Good, "gpg", None), 3, i64::MAX), "a key that never expires");
        assert!(fresh(&check(Outcome::Good, "ssh", None), 9, 5000), "SSH checks don't use the keyring");
        // Expiry for the primary key and the subkey that signed; other subkeys don't count.
        let colons = "pub:u:255:22:AAAA:1:3000::u:::scSC:\nfpr:::::::::PRIMARY:\nuid:u::::1::X::A <a@x>:\n\
                      sub:u:255:22:BBBB:1:2500:::::s:\nfpr:::::::::SIGNER:\nsub:u:255:18:CCCC:1:100:::::e:\nfpr:::::::::OTHER:\n";
        assert_eq!(key_expiry(colons, "PRIMARY", "SIGNER"), Some(2500));
        assert_eq!(key_expiry(colons, "PRIMARY", "PRIMARY"), Some(3000));
        assert_eq!(key_expiry("pub:u:255:22:AAAA:1:::u:::scSC:\nfpr:::::::::PRIMARY:\n", "PRIMARY", "PRIMARY"), None);
        let valid = "[GNUPG:] GOODSIG 4B9A Alice\n[GNUPG:] VALIDSIG SIGNER 2026-10-22 1 0 4 0 22 10 00 PRIMARY\n";
        assert_eq!(signing_subkey(valid), Some("SIGNER"));
    }

    #[test]
    fn tool_timeouts_are_remembered_for_a_minute() {
        let t = check(Outcome::TimedOut, "ssh", None);
        assert!(fresh(&t, 3, 1000 + 59));
        assert!(!fresh(&t, 3, 1000 + 60), "tried again after a minute");
        assert!(!fresh(&check(Outcome::TimedOut, "gpg", None), 4, 1001), "or once the keyring changes");
        assert_eq!(verdict(&t, &[]).status, "unknown", "never shown as Bad");
    }

    #[tokio::test]
    async fn a_tool_that_runs_too_long_times_out() {
        let dir = tempfile::tempdir().unwrap();
        let git = Git::new(dir.path().join("repos"), dir.path().join("home"), 4);
        let s = Signing::with_timeout(dir.path().join("gpg"), Duration::from_millis(50));
        let r = s.tool(&git, "sleep", &["5"], b"", s.timeout).await;
        assert!(matches!(r, Err(Fail::TimedOut)), "{r:?}");
        let r = s.tool(&git, "false", &[], b"", s.timeout).await;
        assert!(r.is_ok_and(|o| !o.status.success()), "a failing tool still answers");
    }

    #[test]
    fn ssh_keys_are_read_without_tools() {
        let k = ssh_key("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFxsPyhOovC3igNqTo8u07KB2pQB9asL+9wpOgr9jf9G my laptop").unwrap();
        assert_eq!(k.fingerprint, "SHA256:tOn8xeW71HpDpSvWJO1qQFsmVDC03PosHw9pOnPgg1M");
        assert_eq!(k.label, "my laptop");
        assert!(ssh_key("ssh-rsa AAAAC3NzaC1lZDI1NTE5AAAAIFxsPyhOovC3igNqTo8u07KB2pQB9asL+9wpOgr9jf9G").is_none(), "type mismatch");
        assert!(ssh_key("ssh-ed25519 AAAA").is_none(), "truncated");
        assert!(ssh_key("ssh-ed25519 /////w==").is_none(), "length past the end");
    }
}
