//! Working with other Gitter servers. Each server has an Ed25519 key, made on first use and kept
//! in its data folder, and publishes the public half at /.well-known/gitter. A request from one
//! server to another carries a signature over the server it's for, its method, path, time and
//! body; the receiver checks it against the key the sending server publishes.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};

use base64::Engine;
use base64::engine::general_purpose::STANDARD as B64;
use ed25519_dalek::{Signature, Signer, SigningKey, VerifyingKey};
use sha2::{Digest, Sha256};

/// The header a signed request carries: `server="https://a.example", ts="1700000000", sig="…"`.
pub const HEADER: &str = "gitter-federation";
/// How far a request's time may be from ours, in seconds.
pub const SKEW: i64 = 300;

static KEYS: LazyLock<Mutex<HashMap<PathBuf, SigningKey>>> = LazyLock::new(Default::default);

/// This server's signing key (one per data folder).
pub fn key(data_dir: &Path) -> std::io::Result<SigningKey> {
    let mut keys = KEYS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(k) = keys.get(data_dir) {
        return Ok(k.clone());
    }
    let file = data_dir.join("federation_ed25519");
    let seed: [u8; 32] = match std::fs::read(&file) {
        Ok(b) => b.try_into().map_err(|_| std::io::Error::other("federation_ed25519 isn't a 32-byte key"))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let seed: [u8; 32] = rand::random();
            // Whole or not at all (a half-written key would be a broken identity); one made
            // meanwhile by another request wins.
            match crate::security::write_new_secret(&file, &seed) {
                Ok(()) => seed,
                Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => std::fs::read(&file)?.try_into().map_err(|_| std::io::Error::other("federation_ed25519 isn't a 32-byte key"))?,
                Err(e) => return Err(e),
            }
        }
        Err(e) => return Err(e),
    };
    let k = SigningKey::from_bytes(&seed);
    keys.insert(data_dir.to_path_buf(), k.clone());
    Ok(k)
}

pub fn public_key(k: &SigningKey) -> String {
    B64.encode(k.verifying_key().to_bytes())
}

pub fn parse_key(s: &str) -> Option<VerifyingKey> {
    let b: [u8; 32] = B64.decode(s.trim()).ok()?.try_into().ok()?;
    VerifyingKey::from_bytes(&b).ok()
}

/// A server's address as servers name each other: `scheme://host[:port]`, lower case, nothing
/// after, without the scheme's default port or a trailing dot (so one server has one name).
pub fn origin(url: &str) -> Option<String> {
    let url = url.trim().trim_end_matches('/');
    let (scheme, rest) = url.split_once("://")?;
    let scheme = scheme.to_ascii_lowercase();
    if !matches!(scheme.as_str(), "http" | "https") || rest.is_empty() || rest.contains(['/', '?', '#', '@', '\\', ' ']) {
        return None;
    }
    let rest = rest.to_ascii_lowercase();
    let default = if scheme == "https" { ":443" } else { ":80" };
    let rest = rest.strip_suffix(default).unwrap_or(&rest);
    let rest = match rest.rsplit_once(':') {
        Some((h, p)) if !h.ends_with(']') || p.is_empty() => format!("{}:{p}", h.trim_end_matches('.')),
        _ => rest.trim_end_matches('.').to_string(),
    };
    (!rest.is_empty() && !rest.starts_with(':')).then(|| format!("{scheme}://{rest}"))
}

/// The host (and port) part of an origin: `a.example`, `127.0.0.1:8080`.
pub fn host(origin: &str) -> &str {
    origin.split_once("://").map_or(origin, |x| x.1)
}

/// What's signed. `to` (the receiving server) keeps a request made for one server from being
/// replayed to another; `nonce` tells two alike requests in one second apart.
fn message(to: &str, method: &str, path: &str, ts: i64, nonce: &str, body: &[u8]) -> Vec<u8> {
    format!("gitter-federation-v1\n{to}\n{}\n{path}\n{ts}\n{nonce}\n{}", method.to_ascii_uppercase(), hex::encode(Sha256::digest(body))).into_bytes()
}

/// The header value for a request from `origin` (this server) to server `to`, signed with `k`.
pub fn sign(k: &SigningKey, origin: &str, to: &str, method: &str, path: &str, ts: i64, body: &[u8]) -> String {
    let nonce = hex::encode(rand::random::<[u8; 16]>());
    let sig = k.sign(&message(to, method, path, ts, &nonce, body));
    format!("server=\"{origin}\", ts=\"{ts}\", nonce=\"{nonce}\", sig=\"{}\"", B64.encode(sig.to_bytes()))
}

/// Whether a request made at `ts` is recent enough at `now`.
pub fn fresh(ts: i64, now: i64) -> bool {
    now.abs_diff(ts) <= SKEW as u64
}

/// A signed request's header: (the server it says it's from, its time, the signature).
pub struct Signed {
    pub server: String,
    pub ts: i64,
    pub nonce: String,
    pub sig: Signature,
    /// The signature as sent, to notice it again (a replay).
    pub raw: String,
}

pub fn parse_header(h: &str) -> Option<Signed> {
    let mut f: HashMap<&str, &str> = HashMap::new();
    for part in h.split(',') {
        let (k, v) = part.trim().split_once('=')?;
        f.insert(k.trim(), v.trim().strip_prefix('"')?.strip_suffix('"')?);
    }
    let raw = f.get("sig")?.to_string();
    let b: [u8; 64] = B64.decode(&raw).ok()?.try_into().ok()?;
    let nonce = f.get("nonce").filter(|n| (16..=64).contains(&n.len()) && n.bytes().all(|c| c.is_ascii_hexdigit()))?.to_string();
    Some(Signed { server: origin(f.get("server")?)?, ts: f.get("ts")?.parse().ok()?, nonce, sig: Signature::from_bytes(&b), raw })
}

/// Whether `s` signs this request, made to server `to` (this one), with `key`.
pub fn verify(key: &VerifyingKey, s: &Signed, to: &str, method: &str, path: &str, body: &[u8]) -> bool {
    key.verify_strict(&message(to, method, path, s.ts, &s.nonce, body), &s.sig).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signatures_cover_method_path_time_and_body() {
        let dir = tempfile::tempdir().unwrap();
        let k = key(dir.path()).unwrap();
        // Kept: the same key comes back, from the file once the cache is gone.
        assert_eq!(key(dir.path()).unwrap().to_bytes(), k.to_bytes());
        KEYS.lock().unwrap().clear();
        assert_eq!(key(dir.path()).unwrap().to_bytes(), k.to_bytes());
        let pk = parse_key(&public_key(&k)).unwrap();
        let (to, path, body) = ("https://b.example", "/api/federation/pulls", b"{\"x\":1}");
        let h = sign(&k, "https://a.example", to, "post", path, 1000, body);
        let s = parse_header(&h).unwrap();
        assert_eq!((s.server.as_str(), s.ts), ("https://a.example", 1000));
        assert!(verify(&pk, &s, to, "POST", path, body));
        assert!(!verify(&pk, &s, to, "POST", path, b"{\"x\":2}"), "body");
        assert!(!verify(&pk, &s, to, "POST", "/api/federation/forks", body), "path");
        assert!(!verify(&pk, &s, to, "GET", path, body), "method");
        assert!(!verify(&pk, &s, "https://c.example", "POST", path, body), "another server");
        let later = parse_header(&h.replace("ts=\"1000\"", "ts=\"1001\"")).unwrap();
        assert!(!verify(&pk, &later, to, "POST", path, body), "time");
        let other = SigningKey::from_bytes(&[7; 32]);
        assert!(!verify(&other.verifying_key(), &s, to, "POST", path, body), "key");
        assert!(parse_header("server=\"https://a.example\", ts=\"1\"").is_none());
        // Two alike requests in one second are told apart.
        assert_ne!(sign(&k, "https://a.example", to, "POST", path, 1000, body), h);
        // The signed text, as other implementations must make it.
        assert_eq!(
            String::from_utf8(message("https://b.example", "post", "/p", 1000, "ab", b"x")).unwrap(),
            "gitter-federation-v1\nhttps://b.example\nPOST\n/p\n1000\nab\n2d711642b726b04401627ca9fbac32f5c8530fb1903cc4db02258717921a4881"
        );
        assert!(fresh(1000, 1300) && fresh(1300, 1000) && !fresh(1000, 1301));
        assert!(!fresh(i64::MIN, 1000) && !fresh(i64::MAX, -1000), "far-off times don't overflow");
    }

    #[test]
    fn origins_are_just_scheme_and_host() {
        assert_eq!(origin("HTTPS://A.Example/").as_deref(), Some("https://a.example"));
        assert_eq!(origin("http://127.0.0.1:8080").as_deref(), Some("http://127.0.0.1:8080"));
        for bad in ["ftp://a.example", "https://a.example/x", "https://u@a.example", "https://", "a.example", "https://a.example?x"] {
            assert!(origin(bad).is_none(), "{bad}");
        }
        assert_eq!(host("https://a.example"), "a.example");
        // One server, one name: default ports and a trailing dot don't make another.
        assert_eq!(origin("https://a.example:443").as_deref(), Some("https://a.example"));
        assert_eq!(origin("http://a.example.:80").as_deref(), Some("http://a.example"));
        assert_eq!(origin("https://a.example:8443").as_deref(), Some("https://a.example:8443"));
        assert_eq!(origin("http://a.example:443").as_deref(), Some("http://a.example:443"));
    }
}
