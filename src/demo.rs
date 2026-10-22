//! `gitter demo`: seed a new data directory with people, repositories, issues,
//! pull requests, reviews, CI runs, a wiki and a discussion, all made through
//! the API like a person would, so the instance looks lived-in from the start.

use axum::body::Body;
use axum::http::Request;
use serde_json::{Value, json};
use tower::ServiceExt;

use crate::{AppState, Config, SignupMode, db::Db};

/// What `seed` made: the accounts (the first is the admin) and their shared password.
pub struct Seeded {
    pub users: Vec<String>,
    pub password: String,
    pub repos: Vec<String>,
}

struct Api {
    app: axum::Router,
    host: String,
}

impl Api {
    /// One request as `cookie`; any answer but 2xx is an error naming the call.
    async fn call(&self, cookie: &str, method: &str, path: &str, body: Value) -> anyhow::Result<(Value, Option<String>)> {
        let req = Request::builder()
            .method(method)
            .uri(path)
            .header("host", &self.host)
            .header("origin", format!("http://{}", self.host))
            .header("cookie", cookie)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))?;
        let resp = self.app.clone().oneshot(req).await?;
        let status = resp.status();
        let cookie = resp
            .headers()
            .get("set-cookie")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.split(';').next())
            .map(String::from);
        let bytes = axum::body::to_bytes(resp.into_body(), 1 << 20).await?;
        let v: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        anyhow::ensure!(status.is_success(), "{method} {path}: {status} {v}");
        Ok((v, cookie))
    }

    async fn post(&self, cookie: &str, path: &str, body: Value) -> anyhow::Result<Value> {
        Ok(self.call(cookie, "POST", &format!("/api{path}"), body).await?.0)
    }
}

/// Seed `cfg.data_dir`, which must be missing or empty (never mixed into real data).
pub async fn seed(cfg: &Config) -> anyhow::Result<Seeded> {
    let dir = &cfg.data_dir;
    if dir.exists() && std::fs::read_dir(dir)?.next().is_some() {
        anyhow::bail!(
            "{} isn't empty: gitter demo only seeds a new data directory (point GITTER_DATA at an empty one)",
            dir.display()
        );
    }
    for d in ["repos", "home"] {
        std::fs::create_dir_all(dir.join(d))?;
    }
    fill(cfg).await.map_err(|e| anyhow::anyhow!("{e}\n(seeding stopped halfway: delete {} before trying again)", dir.display()))
}

async fn fill(cfg: &Config) -> anyhow::Result<Seeded> {
    let dir = &cfg.data_dir;
    let mut cfg = cfg.clone();
    cfg.data_dir = dir.canonicalize()?;
    cfg.signup = SignupMode::Open;
    let st = AppState::new(cfg.clone(), Db::open(&cfg.data_dir.join("gitter.db"))?);
    let api = Api { app: crate::router(st.clone()), host: "demo.local".into() };
    let password = format!("demo-{}", hex::encode(rand::random::<[u8; 12]>()));

    let mut cookies = Vec::new();
    let users = ["alice", "bob", "carol"];
    for (name, display) in users.iter().zip(["Alice Liddell", "Bob Builder", "Carol Danvers"]) {
        let body = json!({ "username": name, "email": format!("{name}@example.com"), "password": password });
        let (_, c) = api.call("", "POST", "/api/auth/signup", body).await?;
        let c = c.ok_or_else(|| anyhow::anyhow!("signing up {name} gave no session"))?;
        api.call(&c, "PATCH", "/api/me", json!({ "display_name": display })).await?;
        cookies.push(c);
    }
    let (alice, bob, carol) = (&cookies[0], &cookies[1], &cookies[2]);

    // Repositories, with a file for each of the rich viewers.
    api.post(alice, "/repos", json!({ "name": "hello", "description": "A tour of Gitter: code, viewers, issues, pull requests, CI and a wiki", "readme": true })).await?;
    let tour = [
        (None, "Add the tour", json!([
            { "path": "README.md", "content": README },
            { "path": "src/main.rs", "content": MAIN_RS },
            { "path": "Cargo.toml", "content": "[package]\nname = \"hello\"\nversion = \"0.1.0\"\nedition = \"2024\"\n" },
        ])),
        (None, "Add sample data, a notebook and a 3D model", json!([
            { "path": "data/planets.csv", "content": PLANETS_CSV },
            { "path": "notebooks/orbits.ipynb", "content": NOTEBOOK },
            { "path": "models/pyramid.stl", "content": PYRAMID_STL },
        ])),
        (None, "Run CI on every push", json!([{ "path": ".gitter/ci.yml", "content": CI_YML }])),
        (Some("faster-parser"), "Parse rows without allocating", json!([{ "path": "src/main.rs", "content": MAIN_RS.replace(".collect::<Vec<_>>()", "") }])),
        (Some("docs"), "Document the viewers", json!([{ "path": "docs/viewers.md", "content": "# Viewers\n\nCSV files show as a table you can sort, notebooks render with their outputs, and STL models turn in 3D.\n" }])),
    ];
    for (new_branch, message, changes) in tour {
        let body = json!({ "branch": "main", "new_branch": new_branch, "message": message, "changes": changes });
        api.post(alice, "/repos/alice/hello/contents", body).await?;
    }
    api.post(alice, "/repos", json!({ "name": "notes", "description": "Plain markdown notes", "readme": true })).await?;
    api.post(bob, "/repos", json!({ "name": "dotfiles", "description": "Bob's shell setup", "readme": true })).await?;
    api.post(carol, "/repos/alice/hello/fork", json!({})).await?;
    for c in [bob, carol] {
        api.post(c, "/repos/alice/hello/star", json!({})).await?;
    }

    // Issues with a conversation; one closed.
    let issues = [
        (bob, "Crash when the CSV has a blank line", "Steps:\n\n1. Add an empty line to `data/planets.csv`\n2. `cargo run`\n\nIt panics on `unwrap()`."),
        (carol, "Show the planets' moons too", "It would be fun to see the moons next to each planet."),
        (alice, "Write a getting started guide", "Cover cloning, the first push and opening a pull request."),
    ];
    for (who, title, body) in issues {
        api.post(who, "/repos/alice/hello/issues", json!({ "title": title, "body": body })).await?;
    }
    api.post(alice, "/repos/alice/hello/issues/1/comments", json!({ "body": "Thanks @bob, I can reproduce it. Blank lines should be skipped." })).await?;
    api.post(bob, "/repos/alice/hello/issues/1/comments", json!({ "body": "Fixed by skipping empty lines, thanks!", "state": "closed" })).await?;
    api.post(alice, "/repos/alice/hello/issues/2/comments", json!({ "body": "Good idea. The CSV needs a moons column first." })).await?;

    // Pull requests: one open with a review, one merged.
    let open = api.post(alice, "/repos/alice/hello/pulls", json!({ "title": "Speed up the CSV parser", "body": "Iterates the fields instead of collecting them.", "head": "faster-parser", "base": "main" })).await?;
    let n = open["number"].as_i64().unwrap_or_default();
    api.post(bob, &format!("/repos/alice/hello/pulls/{n}/reviews"), json!({ "event": "approve", "body": "Nice and simple. Ship it!" })).await?;
    let docs = api.post(alice, "/repos/alice/hello/pulls", json!({ "title": "Document the viewers", "head": "docs", "base": "main" })).await?;
    let m = docs["number"].as_i64().unwrap_or_default();
    api.post(alice, &format!("/repos/alice/hello/pulls/{m}/merge"), json!({ "method": "squash", "delete_branch": true })).await?;

    // Wiki and discussions.
    api.post(alice, "/repos/alice/hello/wiki", json!({ "name": "Home", "content": "# Hello wiki\n\nStart with [Setup](Setup), then read the code.\n" })).await?;
    api.post(alice, "/repos/alice/hello/wiki", json!({ "name": "Setup", "content": "# Setup\n\n```sh\ngit clone http://localhost:3000/alice/hello.git\ncargo run\n```\n" })).await?;
    api.post(carol, "/repos/alice/hello/discussions", json!({ "title": "How do I add my own data?", "body": "Can I drop in another CSV?", "category": "qa" })).await?;
    api.post(alice, "/repos/alice/hello/discussions/1/comments", json!({ "body": "Yes: drag it onto the file list to upload it." })).await?;

    Ok(Seeded {
        users: users.iter().map(|u| u.to_string()).collect(),
        password,
        repos: vec!["alice/hello".into(), "alice/notes".into(), "bob/dotfiles".into(), "carol/hello".into()],
    })
}

/// `gitter demo`: seed, then say how to sign in.
pub async fn run(cfg: &Config) -> anyhow::Result<()> {
    let s = seed(cfg).await?;
    println!("Seeded a demo into {}:", cfg.data_dir.display());
    println!("  repositories: {}", s.repos.join(", "));
    println!("  sign in as {} (the admin), {} with the password {}", s.users[0], s.users[1..].join(" or "), s.password);
    println!("  CI runs are queued and run once the server is up.\n");
    Ok(())
}

const README: &str = "# hello

A small project to show Gitter around.

- `data/planets.csv` opens as a table: click a column to sort.
- `notebooks/orbits.ipynb` renders with its outputs.
- `models/pyramid.stl` turns in 3D: drag to orbit, scroll to zoom.

> [!TIP]
> Drag files anywhere on the code page to upload them.
";

const MAIN_RS: &str = "use std::fs;

fn main() {
    let data = fs::read_to_string(\"data/planets.csv\").unwrap();
    for line in data.lines().skip(1) {
        let fields = line.split(',').collect::<Vec<_>>();
        println!(\"{fields:?}\");
    }
}
";

const PLANETS_CSV: &str = "planet,distance_au,radius_km,moons,type
Mercury,0.39,2440,0,rocky
Venus,0.72,6052,0,rocky
Earth,1.00,6371,1,rocky
Mars,1.52,3390,2,rocky
Jupiter,5.20,69911,95,gas giant
Saturn,9.58,58232,146,gas giant
Uranus,19.22,25362,28,ice giant
Neptune,30.05,24622,16,ice giant
";

const CI_YML: &str = "jobs:
  test:
    steps:
      - name: Say hello
        run: echo \"Hello from Gitter CI\" && ls
";

const PYRAMID_STL: &str = "solid pyramid
facet normal 0 0 -1
 outer loop
  vertex -1 -1 0
  vertex 1 1 0
  vertex 1 -1 0
 endloop
endfacet
facet normal 0 0 -1
 outer loop
  vertex -1 -1 0
  vertex -1 1 0
  vertex 1 1 0
 endloop
endfacet
facet normal 0 -0.894 0.447
 outer loop
  vertex -1 -1 0
  vertex 1 -1 0
  vertex 0 0 2
 endloop
endfacet
facet normal 0.894 0 0.447
 outer loop
  vertex 1 -1 0
  vertex 1 1 0
  vertex 0 0 2
 endloop
endfacet
facet normal 0 0.894 0.447
 outer loop
  vertex 1 1 0
  vertex -1 1 0
  vertex 0 0 2
 endloop
endfacet
facet normal -0.894 0 0.447
 outer loop
  vertex -1 1 0
  vertex -1 -1 0
  vertex 0 0 2
 endloop
endfacet
endsolid pyramid
";

const NOTEBOOK: &str = r##"{
 "nbformat": 4,
 "nbformat_minor": 5,
 "metadata": { "kernelspec": { "name": "python3", "display_name": "Python 3", "language": "python" }, "language_info": { "name": "python" } },
 "cells": [
  { "cell_type": "markdown", "metadata": {}, "source": ["# Orbits\n", "\n", "Kepler's third law: the **period** squared grows with the distance cubed."] },
  { "cell_type": "code", "execution_count": 1, "metadata": {}, "source": ["for name, au in [('Earth', 1.0), ('Mars', 1.52), ('Jupiter', 5.2)]:\n", "    print(f'{name}: {au ** 1.5:.2f} years')"],
    "outputs": [ { "output_type": "stream", "name": "stdout", "text": ["Earth: 1.00 years\n", "Mars: 1.87 years\n", "Jupiter: 11.86 years\n"] } ] },
  { "cell_type": "code", "execution_count": 2, "metadata": {}, "source": ["{'moons': 95}"],
    "outputs": [ { "output_type": "execute_result", "execution_count": 2, "metadata": {}, "data": { "text/plain": ["{'moons': 95}"] } } ] },
  { "cell_type": "code", "execution_count": 3, "metadata": {}, "source": ["show_colours()"],
    "outputs": [ { "output_type": "display_data", "metadata": {}, "data": { "text/plain": ["<Figure>"], "image/png": "iVBORw0KGgoAAAANSUhEUgAAABAAAAAQCAIAAACQkWg2AAABlklEQVR42g3LQQEAIQgAQSMQwQhEIIIRjEAEIhjBCETguU8jGMEId/Of1hrS6A1tWGM0ZsMb0ViN3chGNU7jNl6jNUGELqhgwhCm4EIIS9hCCiUc4QpP/tCRTu9oxzqjMzveic7q7E52qnM6t/P6HxRRuqKKKUOZiiuhLGUrqZRylKs8/YMhRjfUMGMY03AjjGVsI40yjnGNZ38YyKAPdGCDMZgDH8RgDfYgBzU4gzt44w8TmfSJTmwyJnPik5isyZ7kpCZncidv/sERpzvqmDOc6bgTznK2k045x7nO8z8EEvRAAwtGMAMPIljBDjKo4AQ3ePGHhSz6Qhe2GIu58EUs1mIvclGLs7iLt/6wkU3f6MY2YzM3vonN2uxNbmpzNnfz9h8SSXqiiSUjmYknkaxkJ5lUcpKbvPxDIUUvtLBiFLPwIopV7CKLKk5xi1d/OMihH/Rgh3GYBz/EYR32IQ91OId7eOcPF7n0i17sMi7z4pe4rMu+5KUu53Iv7/7hIY/+0Ic9xmM+/BGP9diPfNTjPO7jPT6GdLgQ8hjYogAAAABJRU5ErkJggg==" } } ] }
 ]
}
"##;
