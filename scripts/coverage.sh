#!/usr/bin/env bash
# How much of the code the tests run: the unit tests, the binary tests in tests/ and
# scripts/test-install.sh, which runs the built binary's `taskman update` outside cargo test and so
# isn't counted by `cargo llvm-cov` alone. Writes coverage.json, llvm-cov's JSON summary; other
# arguments go to `cargo llvm-cov report` instead, such as --show-missing-lines.
#
# A binary counts only what it ran when it exits on its own: one killed (as `common::Tui` kills
# taskman when a test ends) writes nothing.
#
# Usage: scripts/coverage.sh [REPORT_OPTIONS...]   (needs cargo-llvm-cov and python3)
set -euo pipefail

cd "$(dirname "$0")/.."
[ "$#" -gt 0 ] || set -- --json --summary-only --output-path coverage.json

# Instrumented builds, in the folder `cargo llvm-cov` uses: in target/ cargo would reuse a plain
# build, as the instrumenting flags aren't part of what it compares. test-install.sh runs the
# taskman that cargo test built for tests/; building it again would give one the tests' profiles
# don't match, and drop what they ran.
export CARGO_TARGET_DIR="$PWD/target/llvm-cov-target"
eval "$(cargo llvm-cov show-env --sh)"
cargo llvm-cov clean --workspace
cargo test --locked
scripts/test-install.sh
cargo llvm-cov report "$@"
