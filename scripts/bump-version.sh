#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"

push=false
args=()
for arg in "$@"; do
  if [[ "$arg" == "--push" ]]; then
    push=true
    continue
  fi
  args+=("$arg")
done

if [[ ${#args[@]} -ne 1 ]] || [[ ! "${args[0]}" =~ ^(major|minor|patch)$ ]]; then
  echo "Usage: $0 [--push] {major|minor|patch}" >&2
  exit 1
fi
level="${args[0]}"

if ! cargo set-version --help &>/dev/null; then
  echo "cargo-edit is required. Install with: cargo install cargo-edit" >&2
  exit 1
fi

cargo set-version --bump "$level"

version="$(cargo pkgid | sed 's/.*#//' | sed 's/^[^@]*@//')"
tag="v${version}"

git add Cargo.toml Cargo.lock
git commit -m "Release ${tag}"
git tag -a "${tag}" -m "Release ${tag}"

branch="$(git rev-parse --abbrev-ref HEAD)"

if [[ "$push" == true ]]; then
  git push origin "$branch"
  git push origin "$tag"
  echo "Pushed ${branch} and ${tag}. Release workflow will build artifacts."
else
  echo "Created commit and tag ${tag}."
  echo "Publish with:"
  echo "  git push origin ${branch}"
  echo "  git push origin ${tag}"
fi
