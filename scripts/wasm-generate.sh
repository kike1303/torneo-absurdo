#!/usr/bin/env bash
# 011 RF1: compiles engine-wasm and writes the four files of wasm-bindgen into `out`. Run from the
# repo root, by `just _wasm-generate` on the reference host and by `just gen-wasm` inside the
# linux/amd64 container elsewhere (which has no `just`, hence a script).
#
#   scripts/wasm-generate.sh <out>
#
# The CLI is wasm-bindgen-cli in the exact version of wasm-bindgen in Cargo.lock. Installing it uses
# the network once per target/: it is outside Cargo.lock, so crate-age and deny do not see its
# dependencies (011 worry 4), and it goes without the RUSTFLAGS below, which are not for it.
# The remap keeps the home of whoever generates out of the binary, and the repo is public. Its own
# target directory: other RUSTFLAGS would rebuild the rest of target/ every time.
set -euo pipefail

out="${1:?usage: scripts/wasm-generate.sh <out>}"
version="$(sed -n '/^name = "wasm-bindgen"$/{n;s/^version = "\(.*\)"$/\1/p;}' Cargo.lock)"
if [ -z "$version" ]; then
    echo "wasm-bindgen is not in Cargo.lock" >&2
    exit 1
fi
cli_root="$PWD/target/wasm-tools/$version"
if [ ! -x "$cli_root/bin/wasm-bindgen" ]; then
    env -u RUSTFLAGS -u CARGO_TARGET_DIR \
        cargo install wasm-bindgen-cli --version "=$version" --locked --root "$cli_root" >&2
fi
RUSTFLAGS="--remap-path-prefix=${CARGO_HOME:-$HOME/.cargo}/registry/src=/cargo --remap-path-prefix=$PWD=/src" \
    CARGO_TARGET_DIR=target/wasm-artifact \
    cargo build -p engine-wasm --target wasm32-unknown-unknown --release --locked
"$cli_root/bin/wasm-bindgen" --target web --omit-default-module-path --out-dir "$out" \
    target/wasm-artifact/wasm32-unknown-unknown/release/engine_wasm.wasm
