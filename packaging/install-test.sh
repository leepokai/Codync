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
    [ -z "${FAKE_HOST_READY:-}" ] ||
      printf '{"binaryPath":"%s"}\n' "$FAKE_HOST_BINARY_PATH" > "$FAKE_HOST_READY"
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
cat > "$tmp/stubs/id" <<'EOF'
#!/bin/sh
[ "${1:-}" = -u ] && echo 1000 || /usr/bin/id "$@"
EOF
cat > "$tmp/stubs/sudo" <<'EOF'
#!/bin/sh
path=$(CDPATH= cd -- "$(dirname "$0")" && pwd)
PATH=$path:/usr/bin:/bin exec "$@"
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
  [ -f "${url#file://}" ] && cat "${url#file://}"
fi
EOF
cat > "$tmp/stubs/gst-inspect-1.0" <<'EOF'
#!/bin/sh
case ${1:-} in
  webrtcbin) [ -f "${GST_WEBRTC:-}" ] ;;
  nicesrc | nicesink) [ -f "${GST_NICE:-}" ] ;;
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
[ "${1:-}" = show ] &&
  printf 'path=%s ;\n' "${FAKE_SERVICE_PATH:-}/codync-host"
exit 0
EOF
cat > "$tmp/stubs/apt-get" <<'EOF'
#!/bin/sh
printf '%s\n' "$*"
EOF
for command in apt-cache update-desktop-database gtk-update-icon-cache xdg-mime gsettings; do
  cat > "$tmp/stubs/$command" <<'EOF'
#!/bin/sh
exit 0
EOF
done
chmod 755 "$tmp/stubs/"*

install() {
  case_name=$1
  app=$2
  shift 2
  shell_override=${SHELL_OVERRIDE:-/bin/sh}
  home=$tmp/$case_name
  mkdir -p "$home"
  bin_dir=${CODYNC_TEST_BIN_DIR:-$home/bin}
  if [ -d "$bin_dir" ]; then
    physical_bin_dir=$(CDPATH= cd -- "$bin_dir" && pwd -P)
  else
    physical_parent=$(CDPATH= cd -- "$(dirname "$bin_dir")" && pwd -P)
    physical_bin_dir=$physical_parent/$(basename "$bin_dir")
  fi
  binary_path=$physical_bin_dir/codync-host
  display=DISPLAY=
  [ "$app" = desktop ] && display=WAYLAND_DISPLAY=wayland-0
  [ "$app" = desktop ] && app_arg=--app || app_arg=
  if [ -z "${FAKE_HOST_BINARY_PATH:-}" ]; then fake_binary_path=$binary_path; else fake_binary_path=$FAKE_HOST_BINARY_PATH; fi
  env -i HOME="$home" USER=test LOGNAME=test SHELL="$shell_override" \
    PATH="$tmp/stubs:/usr/bin:/bin" "$display" XDG_DATA_HOME="$home/data" XDG_CONFIG_HOME="$home/config" \
    CODYNC_RELEASES_URL="file://$tmp/releases" CODYNC_BIN_DIR="$bin_dir" CODYNC_HOME="$home/.codync" \
    FAKE_SERVICE_PATH="$physical_bin_dir" \
    FAKE_HOST_BINARY_PATH="$fake_binary_path" \
    CODYNC_INSTALL_READY_TRIES=1 CODYNC_INSTALL_READY_DELAY=0 DEBIAN_FRONTEND=noninteractive "$@" \
    sh "$root/packaging/install.sh" $app_arg
}

mkdir -p "$tmp/all_plugins/config" "$tmp/missing_webrtc/config" "$tmp/missing_nice/config" "$tmp/zsh/config" "$tmp/not_ready/config" \
  "$tmp/no_display/config" "$tmp/symlink_bin/config"
touch "$tmp/webrtc" "$tmp/nice" "$tmp/pipewire"
install all_plugins desktop GST_WEBRTC="$tmp/webrtc" GST_NICE="$tmp/nice" GST_PIPEWIRE="$tmp/pipewire" \
  CODYNC_INSTALL_HEALTH_URL="file://$tmp/all_ready" FAKE_HOST_READY="$tmp/all_ready" > "$tmp/all.out"
! grep -q "gstreamer1.0-plugins-bad" "$tmp/all.out" ||
  { echo "installer added GStreamer plugins when they were present"; exit 1; }

install missing_webrtc desktop GST_WEBRTC="$tmp/absent-webrtc" GST_NICE="$tmp/nice" GST_PIPEWIRE="$tmp/pipewire" \
  CODYNC_INSTALL_HEALTH_URL="file://$tmp/missing_ready" FAKE_HOST_READY="$tmp/missing_ready" > "$tmp/missing.out"
grep -Fq "libgstreamer-plugins-bad1.0-0 gstreamer1.0-plugins-bad" "$tmp/missing.out" ||
  { echo "installer missed webrtcbin"; exit 1; }
grep -Fq "gstreamer1.0-tools" "$tmp/missing.out" ||
  { echo "installer could not inspect the missing plugin"; exit 1; }

install missing_nice desktop GST_WEBRTC="$tmp/webrtc" GST_NICE="$tmp/absent-nice" GST_PIPEWIRE="$tmp/pipewire" \
  CODYNC_INSTALL_HEALTH_URL="file://$tmp/missing_nice_ready" FAKE_HOST_READY="$tmp/missing_nice_ready" > "$tmp/missing-nice.out"
grep -Fq "gstreamer1.0-nice" "$tmp/missing-nice.out" ||
  { echo "installer missed libnice's GStreamer elements"; exit 1; }

mkdir -p "$tmp/zsh/.local"
CODYNC_TEST_BIN_DIR="$tmp/zsh/.local/bin" SHELL_OVERRIDE=/bin/zsh install zsh desktop \
  GST_WEBRTC="$tmp/webrtc" GST_NICE="$tmp/nice" GST_PIPEWIRE="$tmp/pipewire" \
  CODYNC_INSTALL_HEALTH_URL="file://$tmp/zsh_ready" FAKE_HOST_READY="$tmp/zsh_ready" > "$tmp/zsh.out"
grep -Fq '$HOME/.local/bin' "$tmp/zsh/.zprofile" ||
  { echo "installer did not configure zsh login shells"; exit 1; }
grep -Fq '$HOME/.local/bin' "$tmp/zsh/.zshrc" ||
  { echo "installer did not configure interactive zsh shells"; exit 1; }

install not_ready desktop GST_WEBRTC="$tmp/webrtc" GST_NICE="$tmp/nice" GST_PIPEWIRE="$tmp/pipewire" \
  CODYNC_INSTALL_HEALTH_URL="file://$tmp/never-ready" > "$tmp/not-ready.out"
test -f "$tmp/not_ready/.codync/host-install-failed"
if grep -q 'Next: open Codync' "$tmp/not-ready.out"; then
  echo "installer printed the success hint after the host failed"
  exit 1
fi

install no_display none GST_WEBRTC="$tmp/webrtc" GST_NICE="$tmp/nice" GST_PIPEWIRE="$tmp/pipewire" \
  CODYNC_INSTALL_HEALTH_URL="file://$tmp/no_display_ready" FAKE_HOST_READY="$tmp/no_display_ready" > "$tmp/no-display.out"
grep -Fq "Next: run $tmp/no_display/bin/codync-host pair --port 19222" "$tmp/no-display.out"
if test -f "$tmp/no_display/data/applications/codync.desktop"; then
  echo "installer created a launcher without a display"
  exit 1
fi

mkdir -p "$tmp/symlink_bin/physical"
ln -s physical "$tmp/symlink_bin/bin"
CODYNC_TEST_BIN_DIR="$tmp/symlink_bin/bin" install symlink_bin desktop \
  GST_WEBRTC="$tmp/webrtc" GST_NICE="$tmp/nice" GST_PIPEWIRE="$tmp/pipewire" \
  CODYNC_INSTALL_HEALTH_URL="file://$tmp/symlink_ready" FAKE_HOST_READY="$tmp/symlink_ready" \
  > "$tmp/symlink.out"
if test -f "$tmp/symlink_bin/.codync/host-install-failed"; then
  echo "installer rejected its own host through a symlinked binary directory"
  exit 1
fi
grep -Fq 'Next: open Codync' "$tmp/symlink.out"

echo "installer checks passed"
