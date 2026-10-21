//! Parses `git diff -p` output into files, hunks and lines.

use serde::Serialize;

const FILE_LINE_CAP: usize = 3000;
const TOTAL_LINE_CAP: usize = 40_000;

#[derive(Serialize, Debug, Default)]
pub struct FileDiff {
    pub old_path: String,
    pub new_path: String,
    pub status: &'static str,
    pub binary: bool,
    pub additions: u32,
    pub deletions: u32,
    pub similarity: Option<u32>,
    pub old_mode: Option<String>,
    pub new_mode: Option<String>,
    /// Hunks were dropped because the file (or the whole diff) is too big to show.
    pub too_large: bool,
    pub hunks: Vec<Hunk>,
}

#[derive(Serialize, Debug)]
pub struct Hunk {
    pub header: String,
    pub old_start: u32,
    pub new_start: u32,
    /// [kind, old line no (0 = none), new line no (0 = none), text]; kind is ' ', '+', '-' or '\\'.
    pub lines: Vec<(char, u32, u32, String)>,
}

#[derive(Serialize, Debug, Default)]
pub struct DiffSummary {
    pub files: Vec<FileDiff>,
    pub additions: u32,
    pub deletions: u32,
    pub truncated: bool,
}

/// Undo git's C-style quoting of unusual paths ("a/f\303\266o\tx").
pub fn unquote(s: &str) -> String {
    let Some(inner) = s.strip_prefix('"').and_then(|x| x.strip_suffix('"')) else {
        return s.to_string();
    };
    let mut out: Vec<u8> = Vec::new();
    let b = inner.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'\\' && i + 1 < b.len() {
            let c = b[i + 1];
            match c {
                b'n' => out.push(b'\n'),
                b't' => out.push(b'\t'),
                b'r' => out.push(b'\r'),
                b'"' => out.push(b'"'),
                b'\\' => out.push(b'\\'),
                b'a' => out.push(7),
                b'b' => out.push(8),
                b'f' => out.push(12),
                b'v' => out.push(11),
                b'0'..=b'7' if i + 3 < b.len() => {
                    let oct = &inner[i + 1..i + 4];
                    if let Ok(v) = u8::from_str_radix(oct, 8) {
                        out.push(v);
                        i += 4;
                        continue;
                    }
                    out.push(c);
                }
                _ => out.push(c),
            }
            i += 2;
        } else {
            out.push(b[i]);
            i += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn strip_prefix_path(p: &str, prefix: &str) -> String {
    let p = unquote(p.trim_end_matches('\t'));
    p.strip_prefix(prefix).map(|s| s.to_string()).unwrap_or(p)
}

/// Best-effort split of the `diff --git a/X b/Y` header (used when there are no ---/+++ lines).
fn header_paths(rest: &str) -> (String, String) {
    if rest.starts_with('"') {
        // Quoted: find the closing quote of the first token.
        let mut esc = false;
        for (i, ch) in rest.char_indices().skip(1) {
            if esc {
                esc = false;
                continue;
            }
            if ch == '\\' {
                esc = true;
            } else if ch == '"' {
                let a = &rest[..=i];
                let b = rest[i + 1..].trim_start();
                return (strip_prefix_path(a, "a/"), strip_prefix_path(b, "b/"));
            }
        }
    }
    // Unquoted: when both names are equal, the split is in the middle.
    let n = rest.len();
    if n % 2 == 1 {
        let half = (n - 1) / 2;
        if let (Some(a), Some(b)) = (rest.get(..half), rest.get(half + 1..))
            && let (Some(a2), Some(b2)) = (a.get(2..), b.get(2..))
            && a2 == b2
        {
            return (a2.to_string(), b2.to_string());
        }
    }
    match rest.rfind(" b/") {
        Some(i) => (
            strip_prefix_path(&rest[..i], "a/"),
            rest[i + 3..].to_string(),
        ),
        None => (rest.to_string(), rest.to_string()),
    }
}

fn parse_range(s: &str) -> (u32, u32) {
    let s = s.get(1..).unwrap_or("");
    match s.split_once(',') {
        Some((a, b)) => (a.parse().unwrap_or(0), b.parse().unwrap_or(0)),
        None => (s.parse().unwrap_or(0), 1),
    }
}

pub fn parse(text: &str, input_truncated: bool) -> DiffSummary {
    parse_with(text, input_truncated, FILE_LINE_CAP)
}

/// A diff of one file asked for on its own gets the whole budget.
pub fn parse_one(text: &str, input_truncated: bool) -> DiffSummary {
    parse_with(text, input_truncated, TOTAL_LINE_CAP)
}

fn parse_with(text: &str, input_truncated: bool, file_cap: usize) -> DiffSummary {
    let mut sum = DiffSummary {
        truncated: input_truncated,
        ..Default::default()
    };
    let mut cur: Option<FileDiff> = None;
    let mut old_left = 0u32;
    let mut new_left = 0u32;
    let mut old_no = 0u32;
    let mut new_no = 0u32;
    let mut file_lines = 0usize;
    let mut total_lines = 0usize;

    let finish = |f: Option<FileDiff>, sum: &mut DiffSummary| {
        if let Some(mut f) = f {
            if f.status.is_empty() {
                f.status = if f.old_path != f.new_path {
                    "renamed"
                } else {
                    "modified"
                };
            }
            sum.additions += f.additions;
            sum.deletions += f.deletions;
            sum.files.push(f);
        }
    };

    for line in text.split('\n') {
        // Inside a hunk, every line is content until the counts run out.
        if let Some(f) = cur.as_mut()
            && ((old_left > 0 || new_left > 0) || line.starts_with('\\'))
        {
            let (kind, body) = match line.chars().next() {
                Some(c @ (' ' | '+' | '-' | '\\')) => (c, &line[1..]),
                None => (' ', ""),
                _ => {
                    old_left = 0;
                    new_left = 0;
                    (' ', line)
                }
            };
            if old_left > 0 || new_left > 0 || kind == '\\' {
                let (o, n) = match kind {
                    '+' => {
                        new_left = new_left.saturating_sub(1);
                        f.additions += 1;
                        new_no += 1;
                        (0, new_no)
                    }
                    '-' => {
                        old_left = old_left.saturating_sub(1);
                        f.deletions += 1;
                        old_no += 1;
                        (old_no, 0)
                    }
                    '\\' => (0, 0),
                    _ => {
                        old_left = old_left.saturating_sub(1);
                        new_left = new_left.saturating_sub(1);
                        old_no += 1;
                        new_no += 1;
                        (old_no, new_no)
                    }
                };
                file_lines += 1;
                total_lines += 1;
                if file_lines > file_cap || total_lines > TOTAL_LINE_CAP {
                    f.too_large = true;
                    f.hunks.clear();
                    if total_lines > TOTAL_LINE_CAP {
                        sum.truncated = true;
                    }
                } else if !f.too_large
                    && let Some(h) = f.hunks.last_mut()
                {
                    h.lines.push((kind, o, n, body.to_string()));
                }
                continue;
            }
        }

        if let Some(rest) = line.strip_prefix("diff --git ") {
            finish(cur.take(), &mut sum);
            let (a, b) = header_paths(rest);
            cur = Some(FileDiff {
                old_path: a,
                new_path: b,
                too_large: total_lines > TOTAL_LINE_CAP,
                ..Default::default()
            });
            file_lines = 0;
            continue;
        }
        let Some(f) = cur.as_mut() else { continue };
        if let Some(h) = line.strip_prefix("@@ ") {
            let mut parts = h.split(' ');
            let (os, ol) = parse_range(parts.next().unwrap_or("-0"));
            let (ns, nl) = parse_range(parts.next().unwrap_or("+0"));
            old_left = ol;
            new_left = nl;
            old_no = os.saturating_sub(if ol == 0 { 0 } else { 1 });
            new_no = ns.saturating_sub(if nl == 0 { 0 } else { 1 });
            if ol == 0 {
                old_no = os;
            }
            if nl == 0 {
                new_no = ns;
            }
            let ctx = h
                .split_once("@@")
                .map(|x| x.1)
                .unwrap_or("")
                .trim()
                .to_string();
            if !f.too_large {
                f.hunks.push(Hunk {
                    header: ctx,
                    old_start: os,
                    new_start: ns,
                    lines: Vec::new(),
                });
            }
        } else if let Some(m) = line.strip_prefix("new file mode ") {
            f.status = "added";
            f.new_mode = Some(m.to_string());
        } else if let Some(m) = line.strip_prefix("deleted file mode ") {
            f.status = "deleted";
            f.old_mode = Some(m.to_string());
        } else if let Some(m) = line.strip_prefix("old mode ") {
            f.old_mode = Some(m.to_string());
        } else if let Some(m) = line.strip_prefix("new mode ") {
            f.new_mode = Some(m.to_string());
        } else if let Some(p) = line.strip_prefix("rename from ") {
            f.old_path = unquote(p);
            f.status = "renamed";
        } else if let Some(p) = line.strip_prefix("rename to ") {
            f.new_path = unquote(p);
        } else if let Some(p) = line.strip_prefix("copy from ") {
            f.old_path = unquote(p);
            f.status = "copied";
        } else if let Some(p) = line.strip_prefix("copy to ") {
            f.new_path = unquote(p);
        } else if let Some(s) = line.strip_prefix("similarity index ") {
            f.similarity = s.trim_end_matches('%').parse().ok();
        } else if line.starts_with("Binary files ") || line == "GIT binary patch" {
            f.binary = true;
        } else if let Some(p) = line.strip_prefix("--- ") {
            if p != "/dev/null" {
                f.old_path = strip_prefix_path(p, "a/");
            }
        } else if let Some(p) = line.strip_prefix("+++ ")
            && p != "/dev/null"
        {
            f.new_path = strip_prefix_path(p, "b/");
        }
    }
    finish(cur.take(), &mut sum);
    for f in &mut sum.files {
        if f.status == "added" && f.old_path.is_empty() {
            f.old_path = f.new_path.clone();
        }
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "diff --git a/src/a.rs b/src/a.rs
index 111..222 100644
--- a/src/a.rs
+++ b/src/a.rs
@@ -1,3 +1,4 @@ fn main()
 keep
--- removed line that looks like a header
+added
+++ added line that looks like a header
 tail
diff --git a/new.txt b/new.txt
new file mode 100644
index 000..333
--- /dev/null
+++ b/new.txt
@@ -0,0 +1 @@
+hello
\\ No newline at end of file
diff --git a/old name.txt b/new name.txt
similarity index 90%
rename from old name.txt
rename to new name.txt
diff --git a/img.png b/img.png
index 1..2 100644
Binary files a/img.png and b/img.png differ
diff --git \"a/sp\\303\\266ce.txt\" \"b/sp\\303\\266ce.txt\"
deleted file mode 100644
--- \"a/sp\\303\\266ce.txt\"
+++ /dev/null
@@ -1 +0,0 @@
-bye
";

    #[test]
    fn parses_files_hunks_and_tricky_lines() {
        let d = parse(SAMPLE, false);
        assert_eq!(d.files.len(), 5);
        let a = &d.files[0];
        assert_eq!(a.status, "modified");
        assert_eq!(a.additions, 2);
        assert_eq!(a.deletions, 1);
        assert_eq!(a.hunks[0].header, "fn main()");
        let l = &a.hunks[0].lines;
        assert_eq!(l.len(), 5);
        assert_eq!(
            l[1],
            ('-', 2, 0, "-- removed line that looks like a header".into())
        );
        assert_eq!(l[3].0, '+');
        assert_eq!(l[4], (' ', 3, 4, "tail".into()));

        let n = &d.files[1];
        assert_eq!(n.status, "added");
        assert_eq!(n.new_path, "new.txt");
        assert_eq!(n.hunks[0].lines[0], ('+', 0, 1, "hello".into()));
        assert_eq!(n.hunks[0].lines[1].0, '\\');

        let r = &d.files[2];
        assert_eq!(r.status, "renamed");
        assert_eq!(r.old_path, "old name.txt");
        assert_eq!(r.new_path, "new name.txt");
        assert_eq!(r.similarity, Some(90));

        assert!(d.files[3].binary);
        let q = &d.files[4];
        assert_eq!(q.status, "deleted");
        assert_eq!(q.old_path, "spöce.txt");
        assert_eq!(d.additions, 3);
        assert_eq!(d.deletions, 2);
    }

    #[test]
    fn caps_huge_files() {
        let mut s =
            String::from("diff --git a/big b/big\n--- a/big\n+++ b/big\n@@ -0,0 +1,5000 @@\n");
        for i in 0..5000 {
            s.push_str(&format!("+line {i}\n"));
        }
        let d = parse(&s, false);
        assert!(d.files[0].too_large);
        assert!(d.files[0].hunks.is_empty());
        assert_eq!(d.files[0].additions, 5000);
    }

    #[test]
    fn non_ascii_rename_header_does_not_panic() {
        // "a/éé b/ab" is 11 bytes; the midpoint falls inside a multi-byte char.
        let d = parse(
            "diff --git a/éé b/ab\nsimilarity index 100%\nrename from éé\nrename to ab\n",
            false,
        );
        assert_eq!(d.files[0].old_path, "éé");
        assert_eq!(d.files[0].new_path, "ab");
        let cut = parse("diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ ", true);
        assert_eq!(cut.files.len(), 1);
    }

    #[test]
    fn unquotes() {
        assert_eq!(unquote("\"a\\tb\""), "a\tb");
        assert_eq!(unquote("plain"), "plain");
        assert_eq!(unquote("\"\\303\\266\""), "ö");
    }
}
