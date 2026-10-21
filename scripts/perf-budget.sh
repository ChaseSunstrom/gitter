#!/usr/bin/env bash
# Speed is a tested behaviour: build for release and fail if any page in
# `gitter bench` is slower than its budget (see BUDGETS in src/bench.rs).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -q --release
./target/release/gitter bench --check
