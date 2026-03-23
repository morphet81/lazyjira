#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")"

echo "Building lazyjira (release)..."
cargo build --release

install_dir="${CARGO_HOME:-$HOME/.cargo}/bin"
mkdir -p "$install_dir"
cp target/release/lazyjira "$install_dir/lazyjira"

echo "Installed lazyjira to $install_dir/lazyjira"
