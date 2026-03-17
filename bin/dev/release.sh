#!/usr/bin/env bash
set -euo pipefail

CARGO_TOML="$(git rev-parse --show-toplevel)/Cargo.toml"

current_version=$(grep -m1 '^version = ' "$CARGO_TOML" | sed 's/version = "\(.*\)"/\1/')
echo "Current version: $current_version"

# Days since 2026-01-01 + 100
epoch_2026=$(date -j -f "%Y-%m-%d" "2026-01-01" "+%s")
epoch_today=$(date "+%s")
days=$(( (epoch_today - epoch_2026) / 86400 ))
minor=$(( days + 100 ))

# Find next available patch for today's minor
patch=0
while git rev-parse "v0.${minor}.${patch}" >/dev/null 2>&1; do
  patch=$((patch + 1))
done

new_version="0.${minor}.${patch}"
tag="v$new_version"

echo "Releasing $new_version (tag $tag)"

sed -i '' "s/^version = \"$current_version\"/version = \"$new_version\"/" "$CARGO_TOML"
echo "Updated $CARGO_TOML"

cargo update --workspace
echo "Updated Cargo.lock"

git add "$CARGO_TOML" Cargo.lock
git commit -m "Bump version to $new_version"
git tag -a "$tag" -m "$tag"
git push origin main "$tag"

echo ""
echo "Released $tag"
echo "Watch the build: https://github.com/fabro-sh/fabro/actions"
