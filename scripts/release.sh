#!/usr/bin/env bash
#
# release.sh — release the current commit of `tcnet` to crates.io.
#
# Workflow:
#   1. Pre-flight checks
#        - working tree is clean
#        - HEAD is in sync with its upstream tracking branch
#        - the version in Cargo.toml is not already tagged locally,
#          on `origin`, or on crates.io
#   2. Run the test suite (lib + doc) and `cargo publish --dry-run`.
#   3. Prompt for confirmation (skip with `--yes` / `-y`).
#   4. Tag HEAD with `v<version>` (annotated).
#   5. Run `cargo publish`. If it fails, the local tag is removed
#      and the script aborts.
#   6. Push the tag to `origin`.
#
# Usage:
#   scripts/release.sh
#   scripts/release.sh --yes
#   scripts/release.sh --dry-run

set -euo pipefail

# --- locations ---
script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
crate_dir="$(cd "$script_dir/.." && pwd)"
cd "$crate_dir"

# --- args ---
assume_yes=0
dry_run=0
for arg in "$@"; do
    case "$arg" in
        -y|--yes)  assume_yes=1 ;;
        --dry-run) dry_run=1 ;;
        -h|--help)
            sed -n '2,/^$/p' "$0" | sed 's/^# \{0,1\}//'
            exit 0 ;;
        *) echo "release.sh: unknown argument '$arg'" >&2; exit 1 ;;
    esac
done

# --- helpers ---
die()  { echo "release.sh: $*" >&2; exit 1; }
note() { echo ">>> $*"; }

# --- read crate metadata ---
crate_name=$(grep -m1 '^name'    Cargo.toml | sed -E 's/name *= *"([^"]+)".*/\1/')
version=$(   grep -m1 '^version' Cargo.toml | sed -E 's/version *= *"([^"]+)".*/\1/')
[ -n "$crate_name" ] || die "could not read crate name from Cargo.toml"
[ -n "$version"    ] || die "could not read version from Cargo.toml"
tag="v$version"

note "crate:   $crate_name"
note "version: $version"
note "tag:     $tag"

# --- pre-flight checks ---
note "checking working tree is clean..."
if [ -n "$(git status --porcelain)" ]; then
    git status --short
    die "working tree is not clean — commit or stash your changes first"
fi

note "checking HEAD is pushed to its upstream..."
upstream="$(git rev-parse --abbrev-ref --symbolic-full-name @{u} 2>/dev/null || true)"
[ -n "$upstream" ] || die "current branch has no upstream — push it first"
if [ "$(git rev-parse HEAD)" != "$(git rev-parse "$upstream")" ]; then
    die "HEAD is not in sync with $upstream — push it first"
fi

note "checking tag $tag does not already exist..."
if git rev-parse --verify --quiet "refs/tags/$tag" >/dev/null; then
    die "tag $tag already exists locally"
fi
if git ls-remote --exit-code --tags origin "refs/tags/$tag" >/dev/null 2>&1; then
    die "tag $tag already exists on origin"
fi

note "checking $crate_name@$version is not already on crates.io..."
if command -v curl >/dev/null 2>&1; then
    status=$(curl -fsS -o /dev/null -w '%{http_code}' \
        "https://crates.io/api/v1/crates/$crate_name/$version" 2>/dev/null || true)
    if [ "$status" = "200" ]; then
        die "$crate_name@$version is already published"
    fi
else
    note "(curl not found, skipping crates.io check — cargo publish will catch a duplicate)"
fi

# --- gate work ---
note "running cargo test --lib -- --test-threads=1..."
cargo test --lib -- --test-threads=1

note "running cargo test --doc..."
cargo test --doc

note "running cargo publish --dry-run..."
cargo publish --dry-run

# --- confirmation ---
if [ "$dry_run" -eq 1 ]; then
    note "--dry-run: skipping tag, publish, push"
    exit 0
fi

if [ "$assume_yes" -eq 0 ]; then
    printf '>>> ready to tag %s and publish %s@%s — proceed? [y/N] ' \
        "$tag" "$crate_name" "$version"
    read -r answer
    case "$answer" in
        y|Y|yes|YES) ;;
        *) die "aborted" ;;
    esac
fi

# --- tag ---
note "tagging $tag on $(git rev-parse --short HEAD)..."
git tag -a "$tag" -m "Release $crate_name $version"

# --- publish ---
note "running cargo publish..."
if ! cargo publish; then
    note "cargo publish failed — removing local tag $tag"
    git tag -d "$tag" >/dev/null
    die "publish failed; tag removed"
fi

# --- push tag ---
note "pushing $tag to origin..."
if ! git push origin "refs/tags/$tag"; then
    note "WARNING: $crate_name@$version is live on crates.io but tag push failed"
    note "         retry manually with: git push origin refs/tags/$tag"
    exit 1
fi

note "done. $crate_name@$version is live on crates.io and tagged $tag."
