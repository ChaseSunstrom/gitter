//! Input validation. Every user-supplied name, ref and path goes through here
//! before it reaches git or the filesystem.

const RESERVED: &[&str] = &[
    "about",
    "admin",
    "api",
    "archive",
    "assets",
    "ci",
    "dashboard",
    "explore",
    "favicon.ico",
    "favicon.svg",
    "help",
    "import",
    "login",
    "logout",
    "mcp",
    "new",
    "notifications",
    "org",
    "orgs",
    "packages",
    "raw",
    "robots.txt",
    "runners",
    "search",
    "settings",
    "signup",
    "snippets",
    "static",
    "user",
    "users",
    // The container registry's API.
    "v2",
];

/// User and repository names: start alphanumeric, then alphanumerics, `-`, `_`, `.`.
pub fn name(s: &str, max: usize) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= max
        && b[0].is_ascii_alphanumeric()
        && b.iter()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'-' | b'_' | b'.'))
        && !s.contains("..")
        && !s.to_ascii_lowercase().ends_with(".git")
}

pub fn username(s: &str) -> bool {
    name(s, 39) && !RESERVED.contains(&s.to_ascii_lowercase().as_str())
}

pub fn repo_name(s: &str) -> bool {
    name(s, 100)
}

/// A revision a reader may ask for: a branch, tag, sha or `main~2`-style expression.
pub fn rev(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 255
        && b[0] != b'-'
        && b[0] != b'/'
        && !s.contains("..")
        && !s.contains("@{")
        && !s.contains("//")
        && !s.ends_with('/')
        && b.iter().all(|c| {
            c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-' | b'/' | b'~' | b'^')
        })
}

/// A branch or tag name someone wants to create (mirrors git check-ref-format).
pub fn ref_name(s: &str) -> bool {
    let b = s.as_bytes();
    !b.is_empty()
        && b.len() <= 200
        && s != "HEAD"
        && b[0] != b'-'
        && b.iter()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'-' | b'/'))
        && !s.contains("..")
        && !s.ends_with('.')
        && !s.ends_with(".lock")
        && s.split('/')
            .all(|seg| !seg.is_empty() && !seg.starts_with('.') && !seg.ends_with(".lock"))
}

/// A path inside a repository tree ("" is the root).
pub fn path(s: &str) -> bool {
    if s.is_empty() {
        return true;
    }
    s.len() <= 4096
        && !s.starts_with('/')
        && !s.ends_with('/')
        && !s.chars().any(|c| c.is_control())
        && s.split('/')
            .all(|seg| !seg.is_empty() && seg != "." && seg != "..")
}

/// A path someone wants to write through the web editor.
pub fn write_path(s: &str) -> bool {
    !s.is_empty() && path(s) && s.split('/').all(|seg| !seg.eq_ignore_ascii_case(".git"))
}

pub fn sha(s: &str) -> bool {
    (4..=64).contains(&s.len()) && s.bytes().all(|c| c.is_ascii_hexdigit())
}

pub fn email(s: &str) -> bool {
    s.len() <= 254
        && s.split_once('@')
            .is_some_and(|(a, d)| !a.is_empty() && d.contains('.') && !d.starts_with('.'))
        && !s
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '<' || c == '>')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert!(username("chase"));
        assert!(username("a-b_c.d"));
        assert!(!username("-x"));
        assert!(!username("api"));
        assert!(!username("Settings"));
        assert!(!username("a..b"));
        assert!(!username("x/y"));
        assert!(!repo_name("foo.git"));
        assert!(!repo_name(""));
        assert!(repo_name("my.repo"));
        assert!(!repo_name(&"a".repeat(101)));
    }

    #[test]
    fn revs() {
        assert!(rev("main"));
        assert!(rev("feature/x"));
        assert!(rev("main~2"));
        assert!(rev("abc123"));
        assert!(!rev("--output=/tmp/x"));
        assert!(!rev("-n"));
        assert!(!rev("a..b"));
        assert!(!rev("HEAD@{1}"));
        assert!(!rev("a b"));
        assert!(!rev("a:b"));
    }

    #[test]
    fn ref_names() {
        assert!(ref_name("feature/login"));
        assert!(ref_name("v1.2.0"));
        assert!(!ref_name("HEAD"));
        assert!(!ref_name("-x"));
        assert!(!ref_name("a/.b"));
        assert!(!ref_name("x.lock"));
        assert!(!ref_name("a//b"));
        assert!(!ref_name("a~1"));
        assert!(!ref_name("end."));
    }

    #[test]
    fn paths() {
        assert!(path(""));
        assert!(path("src/main.rs"));
        assert!(!path("/etc/passwd"));
        assert!(!path("../x"));
        assert!(!path("a/../b"));
        assert!(!path("a//b"));
        assert!(!path("a\nb"));
        assert!(write_path("docs/README.md"));
        assert!(!write_path(".git/config"));
        assert!(!write_path("a/.GIT/hooks"));
        assert!(!write_path(""));
    }

    #[test]
    fn misc() {
        assert!(sha("deadbeef"));
        assert!(!sha("xyz"));
        assert!(email("a@b.co"));
        assert!(!email("a@b"));
        assert!(!email("a b@c.d"));
    }
}
