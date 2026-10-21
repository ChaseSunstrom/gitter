//! Two-factor codes (TOTP, RFC 6238), recovery codes and the hash-chained
//! audit log of security events.

use hmac::{Hmac, Mac};
use rand::RngCore;
use rusqlite::{Connection, OptionalExtension};
use serde::Serialize;

use crate::auth::{User, sha256_hex};
use crate::db::{Db, now};
use crate::error::R;

const STEP: i64 = 30;
const B32: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";

pub fn base32(data: &[u8]) -> String {
    let (mut out, mut buf, mut bits) = (String::new(), 0u32, 0);
    for &b in data {
        buf = (buf << 8) | b as u32;
        bits += 8;
        while bits >= 5 {
            out.push(B32[((buf >> (bits - 5)) & 31) as usize] as char);
            bits -= 5;
        }
    }
    if bits > 0 {
        out.push(B32[((buf << (5 - bits)) & 31) as usize] as char);
    }
    out
}

pub fn unbase32(s: &str) -> Option<Vec<u8>> {
    let (mut out, mut buf, mut bits) = (Vec::new(), 0u32, 0);
    for c in s.chars().filter(|c| !c.is_whitespace() && *c != '=') {
        let v = B32.iter().position(|&x| x == c.to_ascii_uppercase() as u8)? as u32;
        buf = (buf << 5) | v;
        bits += 5;
        if bits >= 8 {
            out.push((buf >> (bits - 8)) as u8);
            bits -= 8;
        }
    }
    Some(out)
}

/// A fresh 160-bit TOTP secret, base32 encoded (what authenticator apps expect).
pub fn new_totp_secret() -> String {
    let mut b = [0u8; 20];
    rand::rngs::OsRng.fill_bytes(&mut b);
    base32(&b)
}

pub fn step_now() -> i64 {
    now() / STEP
}

/// The 6-digit code for `secret` at time step `step` (HMAC-SHA1, as every app supports).
pub fn totp_code(secret: &str, step: i64) -> Option<String> {
    let key = unbase32(secret)?;
    let mut mac = Hmac::<sha1::Sha1>::new_from_slice(&key).ok()?;
    mac.update(&(step as u64).to_be_bytes());
    let h = mac.finalize().into_bytes();
    let off = (h[19] & 0xf) as usize;
    let bin = u32::from_be_bytes([h[off] & 0x7f, h[off + 1], h[off + 2], h[off + 3]]);
    Some(format!("{:06}", bin % 1_000_000))
}

/// The step `code` belongs to, if it's valid now (one step of clock drift either
/// way) and newer than `last_step`, so a code can't be used twice.
pub fn check_totp(secret: &str, code: &str, last_step: i64) -> Option<i64> {
    let code: String = code.chars().filter(|c| c.is_ascii_digit()).collect();
    if code.len() != 6 {
        return None;
    }
    let s = step_now();
    (s - 1..=s + 1).find(|&st| {
        st > last_step && totp_code(secret, st).is_some_and(|c| crate::auth::constant_eq(&c, &code))
    })
}

pub fn otpauth_uri(issuer: &str, account: &str, secret: &str) -> String {
    let enc = |s: &str| -> String {
        s.bytes()
            .map(|b| match b {
                b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' => (b as char).to_string(),
                _ => format!("%{b:02X}"),
            })
            .collect()
    };
    format!(
        "otpauth://totp/{}:{}?secret={secret}&issuer={}&algorithm=SHA1&digits=6&period=30",
        enc(issuer),
        enc(account),
        enc(issuer)
    )
}

/// The URI as an SVG QR code for the setup screen.
pub fn qr_svg(uri: &str) -> String {
    qrcode::QrCode::new(uri.as_bytes())
        .map(|q| {
            q.render::<qrcode::render::svg::Color>()
                .min_dimensions(200, 200)
                .quiet_zone(true)
                .build()
        })
        .unwrap_or_default()
}

/// Ten single-use recovery codes like `k7qp-2mxd`, and their hashes for storage.
pub fn new_recovery_codes() -> Vec<(String, String)> {
    const ALPHA: &[u8] = b"abcdefghjkmnpqrstuvwxyz23456789";
    (0..10)
        .map(|_| {
            let mut b = [0u8; 8];
            rand::rngs::OsRng.fill_bytes(&mut b);
            let s: String = b.iter().map(|x| ALPHA[*x as usize % ALPHA.len()] as char).collect();
            let code = format!("{}-{}", &s[..4], &s[4..]);
            let hash = sha256_hex(&code);
            (code, hash)
        })
        .collect()
}

pub fn normalize_recovery(code: &str) -> String {
    let c: String = code.trim().to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    if c.len() == 8 { format!("{}-{}", &c[..4], &c[4..]) } else { c }
}

/// Password-checked sign-ins waiting for their 2FA code, kept in memory for five minutes.
#[derive(Default)]
pub struct Challenges(std::sync::Mutex<std::collections::HashMap<String, (i64, i64, u32)>>);

impl Challenges {
    pub fn issue(&self, user_id: i64) -> String {
        let (token, _) = crate::auth::new_secret("");
        let mut m = self.0.lock().unwrap_or_else(|p| p.into_inner());
        let t = now();
        m.retain(|_, c| c.1 > t);
        if m.len() > 10_000 {
            m.clear();
        }
        m.insert(token.clone(), (user_id, t + 300, 0));
        token
    }

    /// Count one try at a live challenge and return its account. Tries are
    /// counted before the code is checked, so a burst of parallel guesses gets
    /// five in all, not five each.
    pub fn attempt(&self, token: &str) -> Option<i64> {
        let mut m = self.0.lock().unwrap_or_else(|p| p.into_inner());
        let c = m.get_mut(token).filter(|c| c.1 > now())?;
        c.2 += 1;
        let (uid, n) = (c.0, c.2);
        if n >= 5 {
            m.remove(token);
        }
        (n <= 5).then_some(uid)
    }

    pub fn finish(&self, token: &str) {
        self.0.lock().unwrap_or_else(|p| p.into_inner()).remove(token);
    }
}

/// Check a 2FA code (authenticator or recovery code) for `user_id` and use it up.
/// Returns how it was verified, or None.
pub async fn use_code(db: &Db, user_id: i64, code: &str) -> R<Option<String>> {
    let code = code.to_string();
    db.call(move |c| {
        let tx = c.transaction()?;
        let row: Option<(Option<String>, i64)> = tx
            .query_row("SELECT totp_secret, totp_last_step FROM users WHERE id = ?1", [user_id], |r| Ok((r.get(0)?, r.get(1)?)))
            .optional()?;
        let Some((Some(secret), last)) = row else { return Ok(None) };
        if let Some(step) = check_totp(&secret, &code, last) {
            tx.execute("UPDATE users SET totp_last_step = ?1 WHERE id = ?2", rusqlite::params![step, user_id])?;
            tx.commit()?;
            return Ok(Some("authenticator app".to_string()));
        }
        let hash = sha256_hex(&normalize_recovery(&code));
        let used = tx.execute(
            "UPDATE recovery_codes SET used_at = ?1 WHERE user_id = ?2 AND code_hash = ?3 AND used_at IS NULL",
            rusqlite::params![now(), user_id, hash],
        )?;
        if used == 1 {
            let left: i64 = tx.query_row("SELECT count(*) FROM recovery_codes WHERE user_id = ?1 AND used_at IS NULL", [user_id], |r| r.get(0))?;
            tx.commit()?;
            return Ok(Some(format!("recovery code, {left} left")));
        }
        Ok(None)
    })
    .await
}

// ---------- audit log ----------

/// One security event. `subject` is the account it concerns (shown in that
/// person's own log); the actor is whoever did it.
pub struct Event<'a> {
    pub actor: Option<&'a User>,
    pub subject: Option<i64>,
    pub action: &'a str,
    pub detail: String,
    pub ip: &'a str,
}

// Names, not ids, are sealed: ids are only lookup keys and are cleared when an
// account is deleted (SQLite can reuse a deleted account's id).
fn link(prev: &str, at: i64, actor: &str, subject: &str, action: &str, detail: &str, ip: &str) -> String {
    sha256_hex(&format!("{prev}\x1f{at}\x1f{actor}\x1f{subject}\x1f{action}\x1f{detail}\x1f{ip}"))
}

/// Append inside an open connection (the caller's transaction, when it has one).
pub fn append(c: &Connection, e: &Event) -> rusqlite::Result<()> {
    let prev: String = c
        .query_row("SELECT hash FROM audit ORDER BY id DESC LIMIT 1", [], |r| r.get(0))
        .optional()?
        .unwrap_or_default();
    let subject: String = match e.subject {
        Some(id) => c.query_row("SELECT username FROM users WHERE id = ?1", [id], |r| r.get(0)).optional()?.unwrap_or_default(),
        None => String::new(),
    };
    let t = now();
    let actor = e.actor.map(|u| u.username.as_str()).unwrap_or("");
    let hash = link(&prev, t, actor, &subject, e.action, &e.detail, e.ip);
    c.execute(
        "INSERT INTO audit(actor_id, actor, subject_id, subject, action, detail, ip, created_at, hash) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
        rusqlite::params![e.actor.map(|u| u.id), actor, e.subject, subject, e.action, e.detail, e.ip, t, hash],
    )?;
    Ok(())
}

/// Detach a deleted account from the log, so a later account given the same id
/// never sees it. The names stay, so the history still reads correctly.
pub fn forget_user(c: &Connection, id: i64) -> rusqlite::Result<()> {
    c.execute("UPDATE audit SET subject_id = NULL WHERE subject_id = ?1", [id])?;
    c.execute("UPDATE audit SET actor_id = NULL WHERE actor_id = ?1", [id])?;
    Ok(())
}

/// Record an event. Never fails the request it belongs to (logged instead).
pub async fn record(db: &Db, e: Event<'_>) {
    let (actor, subject, action, detail, ip) =
        (e.actor.cloned(), e.subject, e.action.to_string(), e.detail, e.ip.to_string());
    let res = db
        .call(move |c| {
            let e = Event { actor: actor.as_ref(), subject, action: &action, detail, ip: &ip };
            Ok(append(c, &e)?)
        })
        .await;
    if let Err(err) = res {
        eprintln!("audit: {}", err.msg);
    }
}

/// The end of an intact chain. Writing `hash` down elsewhere lets you later
/// prove that no newest entries were removed either.
pub struct Head {
    pub count: usize,
    pub last_id: i64,
    pub hash: String,
}

/// Recompute the chain. Ok(head) when intact, Err(id) of the first altered entry
/// (an entry removed from the middle shows as its successor being altered).
pub fn verify(c: &Connection) -> rusqlite::Result<Result<Head, i64>> {
    let mut s = c.prepare("SELECT id, actor, subject, action, detail, ip, created_at, hash FROM audit ORDER BY id")?;
    let mut rows = s.query([])?;
    let mut head = Head { count: 0, last_id: 0, hash: String::new() };
    while let Some(r) = rows.next()? {
        let id: i64 = r.get(0)?;
        let want = link(&head.hash, r.get(6)?, &r.get::<_, String>(1)?, &r.get::<_, String>(2)?, &r.get::<_, String>(3)?, &r.get::<_, String>(4)?, &r.get::<_, String>(5)?);
        let got: String = r.get(7)?;
        if want != got {
            return Ok(Err(id));
        }
        head = Head { count: head.count + 1, last_id: id, hash: got };
    }
    Ok(Ok(head))
}

#[derive(Serialize)]
pub struct AuditRow {
    pub id: i64,
    #[serde(skip)]
    pub actor_id: Option<i64>,
    pub actor: String,
    pub subject: Option<String>,
    pub action: String,
    pub detail: String,
    pub ip: String,
    pub created_at: i64,
}

/// Newest first; `cond` is a SQL condition over table `a`, `params` its values.
pub fn list(c: &Connection, cond: &str, params: Vec<rusqlite::types::Value>, page: usize) -> R<(Vec<AuditRow>, bool)> {
    let per = 50;
    let mut s = c.prepare(&format!(
        "SELECT a.id, a.actor_id, a.actor, a.subject, a.action, a.detail, a.ip, a.created_at
         FROM audit a WHERE {cond} ORDER BY a.id DESC LIMIT {} OFFSET {}",
        per + 1,
        (page - 1) * per
    ))?;
    let mut rows: Vec<AuditRow> = s
        .query_map(rusqlite::params_from_iter(params), |r| {
            Ok(AuditRow {
                id: r.get(0)?,
                actor_id: r.get(1)?,
                actor: r.get(2)?,
                subject: Some(r.get::<_, String>(3)?).filter(|s| !s.is_empty()),
                action: r.get(4)?,
                detail: r.get(5)?,
                ip: r.get(6)?,
                created_at: r.get(7)?,
            })
        })?
        .collect::<Result<_, _>>()?;
    let more = rows.len() > per;
    rows.truncate(per);
    Ok((rows, more))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn totp_matches_the_rfc_6238_vectors() {
        // RFC 6238 appendix B, SHA1 key "12345678901234567890", 8 digits truncated to our 6.
        let secret = base32(b"12345678901234567890");
        assert_eq!(totp_code(&secret, 59 / 30).unwrap(), "287082");
        assert_eq!(totp_code(&secret, 1111111109 / 30).unwrap(), "081804");
        assert_eq!(totp_code(&secret, 2000000000 / 30).unwrap(), "279037");
        assert_eq!(unbase32(&secret).unwrap(), b"12345678901234567890");
    }

    #[test]
    fn codes_are_single_use_and_drift_tolerant() {
        let s = new_totp_secret();
        let now = step_now();
        let code = totp_code(&s, now).unwrap();
        assert_eq!(check_totp(&s, &code, 0), Some(now));
        assert_eq!(check_totp(&s, &code, now), None, "replay refused");
        let next = totp_code(&s, now + 1).unwrap();
        assert_eq!(check_totp(&s, &format!("{} {}", &next[..3], &next[3..]), now), Some(now + 1));
        assert_eq!(check_totp(&s, "12345", 0), None);
        assert_eq!(normalize_recovery(" K7QP2MXD "), "k7qp-2mxd");
        assert_eq!(new_recovery_codes().len(), 10);
    }

    #[test]
    fn audit_chain_detects_tampering() {
        let c = Connection::open_in_memory().unwrap();
        c.execute_batch("CREATE TABLE users (id INTEGER PRIMARY KEY, username TEXT);
            INSERT INTO users VALUES (1, 'alice');
            CREATE TABLE audit (id INTEGER PRIMARY KEY, actor_id INTEGER, actor TEXT NOT NULL DEFAULT '', subject_id INTEGER, subject TEXT NOT NULL DEFAULT '', action TEXT NOT NULL, detail TEXT NOT NULL DEFAULT '', ip TEXT NOT NULL DEFAULT '', created_at INTEGER NOT NULL, hash TEXT NOT NULL);").unwrap();
        for a in ["login", "2fa_enabled", "repo_deleted", "logout"] {
            append(&c, &Event { actor: None, subject: Some(1), action: a, detail: "x".into(), ip: "1.2.3.4" }).unwrap();
        }
        let head = verify(&c).unwrap().map_err(|_| ()).unwrap();
        assert_eq!((head.count, head.last_id), (4, 4));
        // Detaching a deleted account keeps the seal.
        forget_user(&c, 1).unwrap();
        assert!(verify(&c).unwrap().is_ok());
        c.execute("DELETE FROM audit WHERE id = 2", []).unwrap();
        assert_eq!(verify(&c).unwrap().err(), Some(3), "a removed entry breaks the next link");
        c.execute("UPDATE audit SET detail = 'nothing to see' WHERE id = 1", []).unwrap();
        assert_eq!(verify(&c).unwrap().err(), Some(1));
    }
}
