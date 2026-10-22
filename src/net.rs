//! Outbound HTTP to addresses people type in (webhooks, sign-in providers).
//! Only http and https. The host is looked up once, every address it gives is
//! checked, and the connection goes to exactly those addresses, so a DNS
//! answer that changes after the check (rebinding) can't reach inside. No
//! redirects, no proxies, 10 seconds for the whole exchange, and only the
//! first part of the reply is read (64 KB unless the caller asks for more).

use std::io::Read;
use std::net::{IpAddr, Ipv4Addr};
use std::time::Duration;

use ureq::http::Uri;
use ureq::unversioned::resolver::{DefaultResolver, ResolvedSocketAddrs, Resolver};
use ureq::unversioned::transport::{DefaultConnector, NextTimeout};

pub const TIMEOUT: Duration = Duration::from_secs(10);
/// The most of a reply that is read.
pub const MAX_REPLY: u64 = 64 * 1024;
pub const USER_AGENT: &str = concat!("Gitter/", env!("CARGO_PKG_VERSION"));

pub struct Reply {
    pub status: u16,
    /// At most MAX_REPLY bytes.
    pub body: Vec<u8>,
}

/// Percent-encode everything but the unreserved characters (RFC 3986).
pub fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| if b.is_ascii_alphanumeric() || b"-._~".contains(&b) { (b as char).to_string() } else { format!("%{b:02X}") })
        .collect()
}

/// An http(s) URL with a host and no user name or password in it.
pub fn check_url(url: &str) -> Result<Uri, String> {
    let uri: Uri = url.parse().map_err(|_| "That isn't a valid URL".to_string())?;
    if !matches!(uri.scheme_str(), Some("http" | "https")) {
        return Err("Use an http:// or https:// URL".into());
    }
    let Some(a) = uri.authority().filter(|a| !a.host().is_empty()) else { return Err("The URL needs a host name".into()) };
    if a.as_str().contains('@') {
        return Err("Leave the user name and password out of the URL".into());
    }
    Ok(uri)
}

/// Whether `ip` is an ordinary internet address: not loopback, private,
/// link-local, shared (CGNAT), multicast, unspecified, documentation or
/// otherwise reserved, including IPv4 addresses carried inside IPv6 ones.
pub fn is_public(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(a) => {
            let [x, y, z, _] = a.octets();
            !(a.is_private()
                || a.is_loopback()
                || a.is_link_local()
                || a.is_multicast()
                || a.is_broadcast()
                || a.is_documentation()
                || a.is_unspecified()
                || x == 0
                || x >= 240
                || (x == 100 && (64..128).contains(&y))
                || (x == 192 && y == 0 && z == 0)
                || (x == 198 && (y == 18 || y == 19)))
        }
        IpAddr::V6(a) => {
            if let Some(v4) = a.to_ipv4_mapped() {
                return is_public(v4.into());
            }
            let s = a.segments();
            match s[0] {
                // 6to4 carries an IPv4 address.
                0x2002 => is_public(Ipv4Addr::from(((s[1] as u32) << 16) | s[2] as u32).into()),
                // Teredo and other IETF assignments, and documentation.
                0x2001 if s[1] < 0x200 || s[1] == 0xdb8 => false,
                // Only global unicast; this also refuses ::1, ::, NAT64, ULA, link-local and multicast.
                x => x & 0xe000 == 0x2000,
            }
        }
    }
}

/// One message for "doesn't resolve" and "private", without the address: whoever
/// sets a URL reads it back, and mustn't be able to map internal names.
const NOT_PUBLIC: &str = "Not contacted: the host doesn't resolve to a public address (private or reserved addresses aren't allowed)";

/// For a program that connects by itself (git, through curl): `url`'s host
/// looked up once with every address checked, as a curl resolve entry
/// (`host:port:addr,...`) that makes it connect to exactly those. None when the
/// URL holds an address (already checked: there's nothing to look up). Blocking.
pub fn pin(url: &str) -> Result<Option<String>, String> {
    let uri = check_url(url)?;
    let host = uri.host().unwrap_or_default();
    let port = uri.port_u16().unwrap_or(if uri.scheme_str() == Some("https") { 443 } else { 80 });
    if let Ok(ip) = host.trim_start_matches('[').trim_end_matches(']').parse::<IpAddr>() {
        return if is_public(ip) { Ok(None) } else { Err(NOT_PUBLIC.into()) };
    }
    // curl drops a trailing dot before it looks in its resolve list, so the entry wouldn't match.
    if host.ends_with('.') {
        return Err("Leave the trailing dot out of the host name".into());
    }
    let addrs: Vec<std::net::SocketAddr> = std::net::ToSocketAddrs::to_socket_addrs(&(host, port)).map_err(|_| NOT_PUBLIC.to_string())?.collect();
    if addrs.is_empty() || addrs.iter().any(|a| !is_public(a.ip())) {
        return Err(NOT_PUBLIC.into());
    }
    let list: Vec<String> = addrs.iter().map(|a| if a.is_ipv6() { format!("[{}]", a.ip()) } else { a.ip().to_string() }).collect();
    Ok(Some(format!("{host}:{port}:{}", list.join(","))))
}

/// Resolves like ureq does, but refuses the lot if any address isn't public.
/// ureq connects to what this returns, so the check and the connection can't disagree.
#[derive(Debug)]
struct PublicOnly;

impl Resolver for PublicOnly {
    fn resolve(&self, uri: &Uri, config: &ureq::config::Config, timeout: NextTimeout) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let refuse = || ureq::Error::Io(std::io::Error::other(NOT_PUBLIC));
        let addrs = match DefaultResolver::default().resolve(uri, config, timeout) {
            Ok(a) => a,
            Err(ureq::Error::HostNotFound) => return Err(refuse()),
            Err(e) => return Err(e),
        };
        if addrs.iter().any(|a| !is_public(a.ip())) {
            return Err(refuse());
        }
        Ok(addrs)
    }
}

/// POST `body` to `url`. Blocking: call it from `spawn_blocking`. Any HTTP
/// status is a reply (a redirect is not followed); Err says why there was none.
pub fn post(url: &str, headers: &[(&str, String)], body: &[u8], allow_private: bool) -> Result<Reply, String> {
    fetch(url, headers, Some(body), allow_private, MAX_REPLY)
}

/// POST `body` (or GET without one), reading at most `cap` bytes of the reply.
pub fn fetch(url: &str, headers: &[(&str, String)], body: Option<&[u8]>, allow_private: bool, cap: u64) -> Result<Reply, String> {
    check_url(url)?;
    let cfg = ureq::Agent::config_builder()
        .timeout_global(Some(TIMEOUT))
        .http_status_as_error(false)
        .max_redirects(0)
        .proxy(None)
        .user_agent(USER_AGENT)
        .build();
    let agent = if allow_private {
        ureq::Agent::new_with_config(cfg)
    } else {
        ureq::Agent::with_parts(cfg, DefaultConnector::default(), PublicOnly)
    };
    let sent = match body {
        Some(b) => headers.iter().fold(agent.post(url), |r, (k, v)| r.header(*k, v)).send(b),
        None => headers.iter().fold(agent.get(url), |r, (k, v)| r.header(*k, v)).call(),
    };
    let mut resp = sent.map_err(describe)?;
    let mut out = Vec::new();
    // A reply cut short (a stall past the time limit) still counts by its status.
    let _ = resp.body_mut().as_reader().take(cap).read_to_end(&mut out);
    Ok(Reply { status: resp.status().as_u16(), body: out })
}

fn describe(e: ureq::Error) -> String {
    match e {
        ureq::Error::Io(e) => e.to_string(),
        ureq::Error::Timeout(_) => format!("No answer within {} seconds", TIMEOUT.as_secs()),
        ureq::Error::HostNotFound => "The host name doesn't resolve".into(),
        ureq::Error::ConnectionFailed => "Couldn't connect".into(),
        e => e.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn public(s: &str) -> bool {
        is_public(s.parse().unwrap())
    }

    #[test]
    fn only_public_addresses_pass() {
        for ok in ["1.1.1.1", "8.8.8.8", "100.128.0.1", "198.20.0.1", "2606:4700::1111", "2001:4860:4860::8888", "2002:0808:0808::1"] {
            assert!(public(ok), "{ok} is public");
        }
        for bad in [
            "127.0.0.1", "10.1.2.3", "172.16.0.1", "192.168.1.1", "169.254.169.254", "100.64.0.1", "0.0.0.0", "0.1.2.3",
            "224.0.0.1", "255.255.255.255", "240.0.0.1", "192.0.0.8", "198.18.0.1", "192.0.2.1",
            "::1", "::", "::ffff:127.0.0.1", "::ffff:10.0.0.1", "64:ff9b::a00:1", "fc00::1", "fd12::1", "fe80::1",
            "ff02::1", "2001:db8::1", "2001::1", "2002:7f00:0001::1", "2002:0a00:0001::1",
        ] {
            assert!(!public(bad), "{bad} must be refused");
        }
    }

    #[test]
    fn urls_need_http_a_host_and_no_credentials() {
        assert!(check_url("https://example.com/hook?x=1").is_ok());
        assert!(check_url("http://[::1]:8080/").is_ok(), "addresses are checked at delivery");
        assert!(check_url("ftp://example.com/").is_err());
        assert!(check_url("file:///etc/passwd").is_err());
        assert!(check_url("https://user:pw@example.com/").is_err());
        assert!(check_url("example.com/hook").is_err());
        assert!(check_url("https://exa mple.com/").is_err());
    }

    #[test]
    fn pins_only_public_hosts() {
        assert_eq!(pin("https://1.1.1.1/x.git"), Ok(None));
        for bad in ["http://127.0.0.1/x", "http://[::1]:8080/x", "http://localhost/x", "http://169.254.169.254/", "http://2130706433/", "https://example.com./x"] {
            assert!(pin(bad).is_err(), "{bad} must be refused");
        }
    }
}
