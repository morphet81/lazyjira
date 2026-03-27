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
  echo "  Without --push: asks whether to push branch and tag (when running in a terminal)." >&2
  echo "  With --push: pushes immediately without prompting." >&2
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

do_push() {
  git push origin "$branch"
  git push origin "$tag"
  echo "Pushed ${branch} and ${tag}. Release workflow will build artifacts."
}

if [[ "$push" == true ]]; then
  do_push
elif [[ -t 0 ]]; then
  echo "Created commit and tag ${tag}."
  read -r -p "Push branch and tag to origin? [y/N] " reply
  case "$(printf '%s' "$reply" | tr '[:upper:]' '[:lower:]')" in
    y | yes) do_push ;;
    *)
      echo "Skipped push. When ready:"
      echo "  git push origin ${branch}"
      echo "  git push origin ${tag}"
      ;;
  esac
else
  echo "Created commit and tag ${tag}."
  echo "Non-interactive shell: not pushing. Run:"
  echo "  git push origin ${branch}"
  echo "  git push origin ${tag}"
fi
