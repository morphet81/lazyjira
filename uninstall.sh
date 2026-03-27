#!/usr/bin/env bash
set -euo pipefail

install_dir="${CARGO_HOME:-$HOME/.cargo}/bin"
target="$install_dir/lazyjira"

if [[ ! -e "$target" ]]; then
  echo "lazyjira is not installed at $target (nothing to remove)."
  exit 0
fi

rm -f "$target"
echo "Removed lazyjira from $target"
echo "Per-repository .lazyjira config files were not removed."
