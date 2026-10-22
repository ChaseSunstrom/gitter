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
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Debug)]
enum Outcome {
    Unsigned,
    Good,
    Bad,
    NoKey,
    Expired,
    Revoked,
    Unsupported,
}

/// Why a tool gave no answer.
enum Fail {
    /// Not installed (or too old to know the command).
    Missing,
    /// Failed or timed out this time.
    Broken,
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
        Signing { home, version: AtomicU64::new(0), synced: Default::default() }
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
            _ => Err(Fail::Broken),
        }
    }

    /// Import armored GPG keys into the keyring; returns gpg's status lines.
    async fn gpg_import(&self, git: &Git, keys: &[u8], timeout: Duration) -> Result<String, Fail> {
        self.ensure_home()?;
        let h = self.home.to_string_lossy();
        let args = ["--homedir", &h, "--batch", "--no-tty", "--no-autostart", "--status-fd", "1", "--import"];
        let o = self.tool(git, "gpg", &args, keys, timeout).await?;
        Ok(String::from_utf8_lossy(&o.stdout).into_owned())
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
                if !keys.is_empty() {
                    self.gpg_import(&st.git, keys.join("\n").as_bytes(), Duration::from_secs(300)).await?;
                }
                Ok(())
            })
            .await
            .map(|_| ())
    }

    /// Check one signature over `payload`. Err is the reason it couldn't be checked.
    async fn check(&self, st: &AppState, payload: &[u8], sig: &str) -> Result<(Outcome, &'static str, Option<String>), &'static str> {
        let kind = if sig.starts_with("-----BEGIN SSH SIGNATURE-----") {
            "ssh"
        } else if sig.starts_with("-----BEGIN PGP SIGNATURE-----") {
            "gpg"
        } else {
            return Ok((Outcome::Unsupported, "", None));
        };
        if sig.len() > MAX_SIG {
            return Ok((Outcome::Unsupported, kind, None));
        }
        let res = self.run_check(st, kind, payload, sig).await;
        res.map(|(o, fp)| (o, kind, fp)).map_err(|f| match (f, kind) {
            (Fail::Missing, "ssh") => "This server can't check SSH signatures (it needs ssh-keygen from OpenSSH 8.4 or newer).",
            (Fail::Missing, _) => "This server can't check GPG signatures (gpg isn't installed).",
            (Fail::Broken, _) => "This signature couldn't be checked just now. Reload the page to try again.",
        })
    }

    async fn run_check(&self, st: &AppState, kind: &str, payload: &[u8], sig: &str) -> Result<(Outcome, Option<String>), Fail> {
        if kind == "gpg" {
            self.sync(st).await?;
        }
        self.ensure_home()?;
        let mut b = [0u8; 8];
        rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut b);
        let file = TempFile(self.home.join(format!("sig-{}", hex::encode(b))));
        tokio::fs::write(&file.0, sig).await.map_err(|_| Fail::Broken)?;
        let f = file.0.to_string_lossy();
        let h = self.home.to_string_lossy();
        if kind == "ssh" {
            // Checks the signature against the public key inside it; whose key that is, the database says.
            let args = ["-Y", "check-novalidate", "-n", "git", "-s", &f];
            self.tool(&st.git, "ssh-keygen", &args, payload, TOOL_TIMEOUT).await.and_then(|o| parse_ssh(&o))
        } else {
            let args = ["--homedir", &h, "--batch", "--no-tty", "--no-autostart", "--trust-model", "always", "--status-fd", "1", "--verify", &f, "-"];
            let o = self.tool(&st.git, "gpg", &args, payload, TOOL_TIMEOUT).await?;
            Ok(parse_gpg(&String::from_utf8_lossy(&o.stdout)))
        }
    }
}

/// Removed when dropped, even when a request is abandoned mid-check.
struct TempFile(PathBuf);

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

/// `ssh-keygen -Y check-novalidate` output: `Good "git" signature with ED25519 key SHA256:…`.
fn parse_ssh(o: &std::process::Output) -> Result<(Outcome, Option<String>), Fail> {
    // OpenSSH before 8.4 has no check-novalidate and prints its usage.
    if String::from_utf8_lossy(&o.stderr).contains("usage:") {
        return Err(Fail::Missing);
    }
    let out = String::from_utf8_lossy(&o.stdout);
    let fp = out
        .lines()
        .next()
        .filter(|l| o.status.success() && l.starts_with("Good \"git\" signature"))
        .and_then(|l| l.rsplit_once(" key "))
        .map(|(_, f)| f.trim().to_string())
        .filter(|f| f.starts_with("SHA256:"));
    Ok(match fp {
        Some(f) => (Outcome::Good, Some(f)),
        None => (Outcome::Bad, None),
    })
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
            "BADSIG" => bad = true,
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

/// Check a pasted public key; a GPG key also goes into the keyring (that's how
/// it's read, and harmless: only a database row makes a key count).
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
    let status = st.signing.gpg_import(&st.git, text.as_bytes(), TOOL_TIMEOUT).await.map_err(|f| match f {
        Fail::Missing => AppError::bad("This server can't read GPG keys (gpg isn't installed)"),
        Fail::Broken => AppError::new(axum::http::StatusCode::SERVICE_UNAVAILABLE, "Couldn't read the key just now. Try again."),
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
    for sha in shas.iter().copied().filter(|s| seen.insert(*s)) {
        match st.git.memo.get::<Check>(&key(sha)) {
            Some(c) if c.kind != "gpg" || c.keyring == ver => {
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
                let c = Check { outcome: Outcome::Unsigned, kind: String::new(), fingerprint: None, committer_email, keyring: ver };
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
            Ok((outcome, kind, fingerprint)) => {
                let c = Check { outcome, kind: kind.into(), fingerprint, committer_email, keyring: ver };
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
                Ok(c) => verdict(&c, c.fingerprint.as_ref().and_then(|f| owners.get(f))),
                Err(v) => v,
            };
            (sha, v)
        })
        .collect())
}

fn unknown(why: &str) -> Verification {
    Verification { status: "unknown".into(), reason: why.into(), key_fingerprint: None, signer: None }
}

/// The accounts that registered these fingerprints, with the key's kind.
async fn owners(st: &AppState, fprs: HashSet<String>) -> R<HashMap<String, (String, User)>> {
    if fprs.is_empty() {
        return Ok(HashMap::new());
    }
    let fprs: Vec<String> = fprs.into_iter().collect();
    st.db
        .call(move |c| {
            let marks = vec!["?"; fprs.len()].join(",");
            let mut s = c.prepare(&format!(
                "SELECT k.fingerprint, k.kind, u.id, u.username, u.display_name, u.email, u.is_admin
                 FROM signing_keys k JOIN users u ON u.id = k.user_id WHERE k.fingerprint IN ({marks})"
            ))?;
            let rows = s.query_map(rusqlite::params_from_iter(&fprs), |r| {
                Ok((r.get::<_, String>(0)?, (r.get::<_, String>(1)?, User::from_row(r)?)))
            })?;
            Ok(rows.collect::<Result<HashMap<_, _>, _>>()?)
        })
        .await
}

fn verdict(c: &Check, owner: Option<&(String, User)>) -> Verification {
    let (kind, a) = if c.kind == "ssh" { ("SSH", "an") } else { ("GPG", "a") };
    let says = |status: &str, reason: String, signer: Option<&User>| Verification {
        status: status.into(),
        reason,
        key_fingerprint: c.fingerprint.clone(),
        signer: signer.map(|u| u.username.clone()),
    };
    let unregistered = || says("unverified", format!("Signed with {a} {kind} key that isn't registered to any account here."), None);
    match c.outcome {
        Outcome::Unsigned => Verification { status: "unsigned".into(), reason: String::new(), key_fingerprint: None, signer: None },
        Outcome::Bad => says("unverified", "The signature doesn't match this commit, so the commit may have been changed after it was signed.".into(), None),
        Outcome::Expired => says("unverified", "The signature, or the GPG key that made it, has expired.".into(), None),
        Outcome::Revoked => says("unverified", "The GPG key that made this signature was revoked.".into(), None),
        Outcome::Unsupported => says("unverified", "This kind of signature can't be checked here (only SSH and GPG signatures can).".into(), None),
        Outcome::NoKey => unregistered(),
        Outcome::Good => match owner {
            Some((k, u)) if *k == c.kind => {
                let name = &u.username;
                if owns_email(u, &c.committer_email) {
                    says("verified", format!("Signed with {name}'s {kind} key."), Some(u))
                } else {
                    says("unverified", format!("Signed with {name}'s {kind} key, but the committer email isn't {name}'s."), Some(u))
                }
            }
            _ => unregistered(),
        },
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
    let input = shas.join("\n") + "\n";
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
    fn ssh_keys_are_read_without_tools() {
        let k = ssh_key("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIFxsPyhOovC3igNqTo8u07KB2pQB9asL+9wpOgr9jf9G my laptop").unwrap();
        assert_eq!(k.fingerprint, "SHA256:tOn8xeW71HpDpSvWJO1qQFsmVDC03PosHw9pOnPgg1M");
        assert_eq!(k.label, "my laptop");
        assert!(ssh_key("ssh-rsa AAAAC3NzaC1lZDI1NTE5AAAAIFxsPyhOovC3igNqTo8u07KB2pQB9asL+9wpOgr9jf9G").is_none(), "type mismatch");
        assert!(ssh_key("ssh-ed25519 AAAA").is_none(), "truncated");
        assert!(ssh_key("ssh-ed25519 /////w==").is_none(), "length past the end");
    }
}
