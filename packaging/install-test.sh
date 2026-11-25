#!/bin/sh
# Focused installer checks on Linux. Run from the repository root with:
#   sh packaging/install-test.sh
set -eu

root=$(CDPATH= cd -- "$(dirname "$0")/.." && pwd)
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

mkdir -p "$tmp/releases/codync-host-linux-x86_64" "$tmp/stubs"
mkdir -p "$tmp/releases/latest/download"
cat > "$tmp/releases/codync-host-linux-x86_64/codync-host" <<'EOF'
#!/bin/sh
case ${1:-} in
  --version) echo 2.54.0 ;;
  install)
    [ -z "${FAKE_HOST_READY:-}" ] || : > "$FAKE_HOST_READY"
    ;;
esac
EOF
cat > "$tmp/releases/codync-host-linux-x86_64/codync-screen" <<'EOF'
#!/bin/sh
exit 0
EOF
chmod 755 "$tmp/releases/codync-host-linux-x86_64/"*
tar czf "$tmp/releases/codync-host-linux-x86_64.tar.gz" -C "$tmp/releases" codync-host-linux-x86_64
sha256sum "$tmp/releases/codync-host-linux-x86_64.tar.gz" > "$tmp/releases/codync-host-linux-x86_64.tar.gz.sha256"

cat > "$tmp/releases/codync-linux-x86_64.AppImage" <<'EOF'
#!/bin/sh
mkdir -p squashfs-root/usr/share/icons/hicolor/256x256/apps
printf '\211PNG\r\n\032\n' > squashfs-root/usr/share/icons/hicolor/256x256/apps/codync.png
EOF
chmod 755 "$tmp/releases/codync-linux-x86_64.AppImage"
sha256sum "$tmp/releases/codync-linux-x86_64.AppImage" > "$tmp/releases/codync-linux-x86_64.AppImage.sha256"
mv "$tmp/releases/codync-host-linux-x86_64.tar.gz" "$tmp/releases/codync-host-linux-x86_64.tar.gz.sha256" \
  "$tmp/releases/codync-linux-x86_64.AppImage" "$tmp/releases/codync-linux-x86_64.AppImage.sha256" \
  "$tmp/releases/latest/download/"

cat > "$tmp/stubs/uname" <<'EOF'
#!/bin/sh
[ "${1:-}" = -s ] && echo Linux || echo x86_64
EOF
cat > "$tmp/stubs/curl" <<'EOF'
#!/bin/sh
output=""
url=""
while [ "$#" -gt 0 ]; do
  case $1 in
    -o) output=$2; shift 2 ;;
    -*) shift ;;
    file://*) url=$1; shift ;;
    *) shift ;;
  esac
done
if [ -n "$output" ]; then
  cp "${url#file://}" "$output"
else
  [ -f "${url#file://}" ]
fi
EOF
cat > "$tmp/stubs/gst-inspect-1.0" <<'EOF'
#!/bin/sh
case ${1:-} in
  webrtcbin) [ -f "${GST_WEBRTC:-}" ] ;;
  pipewiresrc) [ -f "${GST_PIPEWIRE:-}" ] ;;
  *) exit 1 ;;
esac
EOF
cat > "$tmp/stubs/ldd" <<'EOF'
#!/bin/sh
exit 0
EOF
cat > "$tmp/stubs/ldconfig" <<'EOF'
#!/bin/sh
echo 'libfuse.so.2'
EOF
cat > "$tmp/stubs/systemctl" <<'EOF'
#!/bin/sh
[ "${1:-}" = --user ] && shift
[ "${1:-}" = cat ] && exit 1
exit 0
EOF
cat > "$tmp/stubs/apt-get" <<'EOF'
#!/bin/sh
printf '%s\n' "$*"
EOF
chmod 755 "$tmp/stubs/"*

install() {
  case_name=$1
  shift
  home=$tmp/$case_name
  mkdir -p "$home"
  env -i HOME="$home" USER=test LOGNAME=test SHELL=/bin/sh \
    PATH="$tmp/stubs:/usr/bin:/bin" WAYLAND_DISPLAY=wayland-0 XDG_DATA_HOME="$home/data" XDG_CONFIG_HOME="$home/config" \
    CODYNC_RELEASES_URL="file://$tmp/releases" CODYNC_BIN_DIR="$home/bin" CODYNC_HOME="$home/.codync" \
    CODYNC_INSTALL_READY_TRIES=1 CODYNC_INSTALL_READY_DELAY=0 DEBIAN_FRONTEND=noninteractive "$@" \
    sh "$root/packaging/install.sh" --app
}

mkdir -p "$tmp/all_plugins/config" "$tmp/missing_webrtc/config" "$tmp/not_ready/config"
touch "$tmp/webrtc" "$tmp/pipewire"
install all_plugins GST_WEBRTC="$tmp/webrtc" GST_PIPEWIRE="$tmp/pipewire" \
  CODYNC_INSTALL_HEALTH_URL="file://$tmp/all_ready" FAKE_HOST_READY="$tmp/all_ready" > "$tmp/all.out"
! grep -q "gstreamer1.0-plugins-bad" "$tmp/all.out" ||
  { echo "installer added webrtcbin when it was present"; exit 1; }

install missing_webrtc GST_WEBRTC="$tmp/absent-webrtc" GST_PIPEWIRE="$tmp/pipewire" \
  CODYNC_INSTALL_HEALTH_URL="file://$tmp/missing_ready" FAKE_HOST_READY="$tmp/missing_ready" > "$tmp/missing.out"
grep -Fq "libgstreamer-plugins-bad1.0-0 gstreamer1.0-plugins-bad" "$tmp/missing.out" ||
  { echo "installer missed webrtcbin"; exit 1; }
grep -Fq "gstreamer1.0-tools" "$tmp/missing.out" ||
  { echo "installer could not inspect the missing plugin"; exit 1; }

install not_ready GST_WEBRTC="$tmp/webrtc" GST_PIPEWIRE="$tmp/pipewire" \
  CODYNC_INSTALL_HEALTH_URL="file://$tmp/never-ready" > "$tmp/not-ready.out"
test -f "$tmp/not_ready/.codync/host-install-failed"
! grep -q 'Next: open Codync' "$tmp/not-ready.out"

echo "installer checks passed"
