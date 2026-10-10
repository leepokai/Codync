#!/usr/bin/env bash
# Prints the GitHub release description for a tag: the Summary of the PR that merged it into
# main (or the PR's opening text when it has no Summary heading), else the feat/fix commits
# since the previous tag. Auto Tag passes it to `gh release create`. Usage: release-notes.sh v2.13.2
set -euo pipefail
tag=$1
sha=$(git rev-list -n1 "$tag")
pr=$(gh api "repos/{owner}/{repo}/commits/$sha/pulls" -q '.[0].number' || true)
notes=""
if [ -n "$pr" ]; then
  notes=$(gh pr view "$pr" --json body -q .body | tr -d '\r' |
    awk 'BEGIN{p=1} /^🤖/{exit} /^## /{p=($0=="## Summary"); next} p' |
    grep -v -i -E '^- (build: )?(version|bump) ' | sed -e '/./,$!d' || true)
fi
if [ -z "${notes//[[:space:]]/}" ]; then
  prev=$(git describe --tags --abbrev=0 "$tag^")
  notes=$(git log --no-merges --format='- %s' "$prev..$tag" | grep -E '^- (feat|fix)(\(|:|!)' |
    grep -v '(web)' | awk '!seen[$0]++' || true)
fi
printf '%s\n' "$notes"
