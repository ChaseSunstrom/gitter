//! Language detection by file name, used for the language bar and a repo's
//! primary language.

use std::collections::HashMap;
use std::path::Path;

use crate::AppState;

/// (name, color, extensions or exact file names)
pub const LANGS: &[(&str, &str, &[&str])] = &[
    ("Rust", "#dea584", &["rs"]),
    ("TypeScript", "#3178c6", &["ts", "tsx", "mts", "cts"]),
    ("JavaScript", "#f1e05a", &["js", "jsx", "mjs", "cjs"]),
    ("Python", "#3572a5", &["py", "pyi", "pyw"]),
    ("Go", "#00add8", &["go"]),
    ("Java", "#b07219", &["java"]),
    ("Kotlin", "#a97bff", &["kt", "kts"]),
    ("Swift", "#f05138", &["swift"]),
    ("C", "#555555", &["c", "h"]),
    ("C++", "#f34b7d", &["cpp", "cc", "cxx", "hpp", "hh", "hxx"]),
    ("C#", "#178600", &["cs"]),
    ("Ruby", "#701516", &["rb", "rake", "gemspec"]),
    ("PHP", "#4f5d95", &["php"]),
    ("Shell", "#89e051", &["sh", "bash", "zsh", "fish"]),
    ("HTML", "#e34c26", &["html", "htm"]),
    ("CSS", "#663399", &["css"]),
    ("SCSS", "#c6538c", &["scss", "sass"]),
    ("Vue", "#41b883", &["vue"]),
    ("Svelte", "#ff3e00", &["svelte"]),
    ("Dart", "#00b4ab", &["dart"]),
    ("Elixir", "#6e4a7e", &["ex", "exs"]),
    ("Erlang", "#b83998", &["erl", "hrl"]),
    ("Haskell", "#5e5086", &["hs"]),
    ("Scala", "#c22d40", &["scala", "sc"]),
    ("Clojure", "#db5855", &["clj", "cljs", "cljc", "edn"]),
    ("Lua", "#000080", &["lua"]),
    ("Zig", "#ec915c", &["zig"]),
    ("Nim", "#ffc200", &["nim"]),
    ("OCaml", "#ef7a08", &["ml", "mli"]),
    ("R", "#198ce7", &["r"]),
    ("Julia", "#a270ba", &["jl"]),
    ("Perl", "#0298c3", &["pl", "pm"]),
    ("Objective-C", "#438eff", &["m", "mm"]),
    ("SQL", "#e38c00", &["sql"]),
    ("Dockerfile", "#384d54", &["Dockerfile", "dockerfile"]),
    ("Makefile", "#427819", &["Makefile", "makefile", "mk"]),
    ("Nix", "#7e7eff", &["nix"]),
    ("Terraform", "#7b42bc", &["tf", "hcl"]),
    ("PowerShell", "#012456", &["ps1", "psm1"]),
    ("Vim Script", "#199f4b", &["vim"]),
    ("Solidity", "#aa6746", &["sol"]),
    ("Assembly", "#6e4c13", &["asm", "s"]),
    ("Fortran", "#4d41b1", &["f", "f90", "f95"]),
    ("Groovy", "#4298b8", &["groovy", "gradle"]),
    ("F#", "#b845fc", &["fs", "fsx"]),
    ("Crystal", "#000100", &["cr"]),
    ("V", "#4f87c4", &["v"]),
    ("Elm", "#60b5cc", &["elm"]),
    ("GLSL", "#5686a5", &["glsl", "vert", "frag"]),
    ("WebAssembly", "#04133b", &["wat", "wast"]),
];

pub fn detect(path: &str) -> Option<(&'static str, &'static str)> {
    let file = path.rsplit('/').next().unwrap_or(path);
    let ext = file.rsplit_once('.').map(|(_, e)| e).unwrap_or(file);
    for (name, color, exts) in LANGS {
        if exts
            .iter()
            .any(|e| *e == file || e.eq_ignore_ascii_case(ext) && file.contains('.'))
        {
            return Some((name, color));
        }
    }
    None
}

fn vendored(path: &str) -> bool {
    path.split('/').any(|s| {
        matches!(
            s,
            "node_modules" | "vendor" | "third_party" | "dist" | "build" | ".yarn" | "target"
        )
    }) || path.ends_with(".min.js")
        || path.ends_with(".min.css")
}

/// Bytes per language at `rev`, largest first; None when git failed.
pub async fn stats(st: &AppState, path: &Path, rev: &str) -> Option<Vec<(String, String, u64)>> {
    // ponytail: past 32 MB of listing (~300k files) the stats cover only the first files; fine for a bar.
    let args = ["ls-tree", "-r", "-l", "-z", rev];
    let out = st.git.run_records(path, &args, 32 << 20, 0).await.ok()?;
    let mut m: HashMap<&'static str, (&'static str, u64)> = HashMap::new();
    for rec in out.split(|b| *b == 0) {
        let rec = String::from_utf8_lossy(rec);
        let Some((meta, p)) = rec.split_once('\t') else {
            continue;
        };
        let mut it = meta.split_whitespace();
        let (_, kind, _, size) = (it.next(), it.next(), it.next(), it.next());
        if kind != Some("blob") || vendored(p) {
            continue;
        }
        let size: u64 = size.and_then(|s| s.parse().ok()).unwrap_or(0);
        if let Some((name, color)) = detect(p) {
            let e = m.entry(name).or_insert((color, 0));
            e.1 += size;
        }
    }
    let mut v: Vec<_> = m
        .into_iter()
        .map(|(n, (c, s))| (n.to_string(), c.to_string(), s))
        .collect();
    v.sort_by_key(|x| std::cmp::Reverse(x.2));
    Some(v)
}

/// Recompute and store a repo's primary language (default branch).
pub async fn refresh(st: &AppState, id: i64, path: &Path) {
    let Some(v) = stats(st, path, "HEAD").await else { return };
    let lang = v.first().map(|x| x.0.clone()).unwrap_or_default();
    let _ = st
        .db
        .call(move |c| {
            Ok(c.execute(
                "UPDATE repos SET language = ?1 WHERE id = ?2",
                rusqlite::params![lang, id],
            )?)
        })
        .await;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects() {
        assert_eq!(detect("src/main.rs").unwrap().0, "Rust");
        assert_eq!(detect("Dockerfile").unwrap().0, "Dockerfile");
        assert_eq!(detect("a/b/App.TSX").unwrap().0, "TypeScript");
        assert!(detect("README").is_none());
        assert!(detect("notes.txt").is_none());
        assert!(vendored("web/node_modules/x/index.js"));
        assert!(!vendored("src/index.js"));
    }
}
