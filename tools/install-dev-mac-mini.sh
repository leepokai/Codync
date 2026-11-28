#!/usr/bin/env bash
# Build Codync Dev on this Mac and replace the install on the Mac mini (ssh host `mac-mini`).
# Touches only Codync Dev (com.pokai.Codync.dev): never production, never its data directory.
# Usage: tools/install-dev-mac-mini.sh [--no-build]
set -euo pipefail

REMOTE=mac-mini
APP_NAME="Codync Dev"
BUNDLE_ID=com.pokai.Codync.dev
HOST_LABEL=com.pokai.codync.dev.host
PORT=19223

root=$(cd "$(dirname "$0")/.." && pwd)
app="$root/apps/desktop/dist/mac-arm64/$APP_NAME.app"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# Key-only, host key must already be known, no prompts: a surprise on the wire fails the run.
ssh_opts=(-o BatchMode=yes -o StrictHostKeyChecking=yes -o ConnectTimeout=10)
# shellcheck disable=SC2029  # expanded here on purpose: the values are this script's constants
rssh() { ssh "${ssh_opts[@]}" "$REMOTE" "$@"; }

if [[ "${1:-}" != "--no-build" ]]; then
  (cd "$root/apps/desktop" && npm run dist:mac:dev)
fi

[[ -d "$app" ]] || { echo "missing $app (build first)" >&2; exit 1; }
codesign --verify --deep --strict "$app"
local_host_sha=$(shasum -a 256 "$app/Contents/Resources/codync-host" | cut -d' ' -f1)

ditto -c -k --keepParent "$app" "$work/app.zip"
local_zip_sha=$(shasum -a 256 "$work/app.zip" | cut -d' ' -f1)

rssh true  # fail early if the Mac mini is unreachable or the key is refused

remote_dir=$(rssh 'mktemp -d')
scp "${ssh_opts[@]}" "$work/app.zip" "$REMOTE:$remote_dir/app.zip" || { rssh "rm -rf '$remote_dir'" || true; exit 1; }

# Refuse to install anything that is not byte-identical to what was built and signed here.
rssh "set -e
trap 'rm -rf \"$remote_dir\"' EXIT
cd '$remote_dir'
[ \"\$(shasum -a 256 app.zip | cut -d' ' -f1)\" = '$local_zip_sha' ]
ditto -x -k app.zip .
codesign --verify --deep --strict '$APP_NAME.app'
[ \"\$(shasum -a 256 '$APP_NAME.app/Contents/Resources/codync-host' | cut -d' ' -f1)\" = '$local_host_sha' ]
xattr -dr com.apple.quarantine '$APP_NAME.app'
# Quit only Codync Dev and its host, once the new build checked out; the data directory stays.
osascript -e 'tell application id \"$BUNDLE_ID\" to quit' 2>/dev/null || true
pkill -x '$APP_NAME' || true
launchctl bootout gui/\$(id -u)/$HOST_LABEL 2>/dev/null || true
rm -rf '/Applications/$APP_NAME.app'
mv '$APP_NAME.app' /Applications/
'/Applications/$APP_NAME.app/Contents/Resources/codync-host' install --port $PORT
open -a '/Applications/$APP_NAME.app'"

# Installed means the new host answers with the binary built here, not that files were copied.
for _ in $(seq 20); do
  if health=$(rssh "curl -fsS http://127.0.0.1:$PORT/health" 2>/dev/null) && [[ $health == *"$local_host_sha"* ]]; then
    echo "installed on $REMOTE: $health"
    exit 0
  fi
  sleep 1
done
echo "installed, but /health on $REMOTE:$PORT never answered with the new host's hash" >&2
exit 1
