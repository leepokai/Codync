#!/bin/sh
# Codync installer — curl -fsSL https://raw.githubusercontent.com/leepokai/Codync/main/packaging/install.sh | sh
#
#   macOS: the Codync app (the host ships inside it)
#   Linux: codync-host, plus the desktop app when a display is present
#
# Options (after `sh -s --`):
#   --host-only   only codync-host (headless Macs, servers)
#   --app         Linux: also install the desktop app without a display
# Environment:
#   CODYNC_VERSION  a release tag such as v2.2.1 (default: latest)
#   CODYNC_RELEASES_URL  another release channel with GitHub's layout (dev cloud computers: the dev channel, which
#                   has the host only: use it with --host-only)
#   CODYNC_BIN_DIR  where binaries go (default: ~/.local/bin, /usr/local/bin as root)
set -eu

HOST_ONLY=0
FORCE_APP=0
for arg in "$@"; do
  case $arg in
    --host-only) HOST_ONLY=1 ;;
    --app) FORCE_APP=1 ;;
    *) echo "Unknown option: $arg" >&2; exit 2 ;;
  esac
done

say() { printf '\033[1m==>\033[0m %s\n' "$*"; }
die() { printf '\033[31merror:\033[0m %s\n' "$*" >&2; exit 1; }

RELEASES=${CODYNC_RELEASES_URL:-https://github.com/leepokai/Codync/releases}
RELEASES=${RELEASES%/}
VERSION=${CODYNC_VERSION:-latest}
if [ "$VERSION" = latest ]; then BASE=$RELEASES/latest/download; else BASE=$RELEASES/download/v${VERSION#v}; fi

case $(uname -s) in
  Darwin) OS=macos ;;
  Linux) OS=linux ;;
  *) die "Codync supports macOS and Linux." ;;
esac
case $(uname -m) in
  arm64 | aarch64) ARCH=arm64 ;;
  x86_64 | amd64) ARCH=x86_64 ;;
  *) die "Unsupported CPU: $(uname -m) (x86_64 and arm64 only)." ;;
esac

if [ "$(id -u)" = 0 ]; then BIN_DIR=${CODYNC_BIN_DIR:-/usr/local/bin}; else BIN_DIR=${CODYNC_BIN_DIR:-$HOME/.local/bin}; fi
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT

# Downloads a release asset into $TMP and checks it against its .sha256.
fetch() {
  say "Downloading $1"
  curl -fL --progress-bar -o "$TMP/$1" "$BASE/$1" || die "download failed: $BASE/$1"
  curl -fsSL -o "$TMP/$1.sha256" "$BASE/$1.sha256" || die "checksum missing: $BASE/$1.sha256"
  want=$(cut -d' ' -f1 "$TMP/$1.sha256")
  if command -v sha256sum >/dev/null; then got=$(sha256sum "$TMP/$1" | cut -d' ' -f1); else got=$(shasum -a 256 "$TMP/$1" | cut -d' ' -f1); fi
  [ "$want" = "$got" ] || die "checksum mismatch for $1"
}

# Replaces $2 with $1 by rename, so a running binary is never overwritten in place.
place() {
  mkdir -p "$(dirname "$2")"
  cp "$1" "$2.new" && chmod 755 "$2.new" && mv -f "$2.new" "$2"
}

install_host() {
  name=codync-host-$OS-$ARCH
  fetch "$name.tar.gz"
  tar xzf "$TMP/$name.tar.gz" -C "$TMP"
  place "$TMP/$name/codync-host" "$BIN_DIR/codync-host"
  # Linux: the Remote screen helper, which the host starts from beside itself.
  if [ -f "$TMP/$name/codync-screen" ]; then place "$TMP/$name/codync-screen" "$BIN_DIR/codync-screen"; fi
  # Linux: the computer-use driver bots act through, also started from beside the host.
  if [ -f "$TMP/$name/cua-driver" ]; then place "$TMP/$name/cua-driver" "$BIN_DIR/cua-driver"; fi
  say "Installed $("$BIN_DIR/codync-host" --version) to $BIN_DIR"
  restart_service "$BIN_DIR/codync-host"
}

# Restarts the background host if its service runs the binary at $1.
restart_service() {
  if [ $OS = linux ]; then
    unit=${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/codync-host.service
    grep -qF "$1" "$unit" 2>/dev/null && systemctl --user restart codync-host && say "Restarted the codync-host service"
  else
    grep -qF "$1" "$HOME/Library/LaunchAgents/com.pokai.codync.host.plist" 2>/dev/null &&
      launchctl kickstart -k "gui/$(id -u)/com.pokai.codync.host" && say "Restarted the codync-host service"
  fi
  return 0
}

install_mac_app() {
  fetch codync-macos.dmg
  mnt=$TMP/mnt
  hdiutil attach -quiet -nobrowse -readonly -mountpoint "$mnt" "$TMP/codync-macos.dmg"
  osascript -e 'tell application id "com.pokai.Codync" to quit' >/dev/null 2>&1 || true
  sudo=""
  [ -w /Applications ] || sudo=sudo
  $sudo rm -rf /Applications/Codync.app
  $sudo ditto "$mnt/Codync.app" /Applications/Codync.app
  hdiutil detach -quiet "$mnt"
  mkdir -p "$BIN_DIR" && ln -sf /Applications/Codync.app/Contents/Resources/codync-host "$BIN_DIR/codync-host"
  say "Installed Codync.app to /Applications"
  restart_service /Applications/Codync.app/Contents/Resources/codync-host
  open /Applications/Codync.app
}

# Adds the launcher to GNOME's Dock once, replacing only its old identity.
pin_gnome_launcher() {
  command -v gsettings >/dev/null || return 0
  command -v python3 >/dev/null || return 0
  case ${XDG_CURRENT_DESKTOP:-} in
    *GNOME* | *Unity*) ;;
    *) return 0 ;;
  esac
  favorites=$(gsettings get org.gnome.shell favorite-apps 2>/dev/null) || return 0
  updated=$(python3 - "$favorites" <<'PY'
import ast
import sys

value = sys.argv[1]
if value.startswith("@as "):
    value = value[5:]
favorites = ast.literal_eval(value)
if not isinstance(favorites, list) or not all(isinstance(item, str) for item in favorites):
    raise SystemExit(2)
updated = []
launcher_added = False
for favorite in favorites:
    if favorite == "com.pokai.Codync.desktop":
        continue
    if favorite == "codync.desktop":
        if launcher_added:
            continue
        launcher_added = True
    updated.append(favorite)
if not launcher_added:
    updated.append("codync.desktop")
print(repr(updated))
PY
  ) || return 0
  [ "$updated" = "$favorites" ] ||
    gsettings set org.gnome.shell favorite-apps "$updated" ||
    say "Couldn't pin Codync to the Dock"
}

install_linux_app() {
  name=codync-linux-$ARCH
  fetch "$name.AppImage"
  place "$TMP/$name.AppImage" "$BIN_DIR/codync"
  data=${XDG_DATA_HOME:-$HOME/.local/share}
  [ "$(id -u)" = 0 ] && data=/usr/local/share
  mkdir -p "$data/applications"
  icons=$data/icons/hicolor/256x256/apps
  mkdir -p "$icons"
  # The AppImage carries the product icon. Its squashfs can be read without FUSE.
  (
    cd "$TMP"
    "$BIN_DIR/codync" --appimage-extract usr/share/icons/hicolor/256x256/apps/codync.png >/dev/null
  ) || die "Couldn't extract Codync's icon from the AppImage."
  cp "$TMP/squashfs-root/usr/share/icons/hicolor/256x256/apps/codync.png" "$icons/com.pokai.Codync.png"
  cat > "$data/applications/codync.desktop" <<EOF
[Desktop Entry]
Version=1.0
Type=Application
Name=Codync
Comment=Your coding agents as teammates
Exec=$BIN_DIR/codync %U
TryExec=$BIN_DIR/codync
Icon=com.pokai.Codync
Terminal=false
Categories=Development;
MimeType=x-scheme-handler/codync;x-scheme-handler/com.pokai.codync;
StartupWMClass=Codync
StartupNotify=true
X-GNOME-UsesNotifications=true
EOF
  rm -f "$data/applications/com.pokai.Codync.desktop"
  if command -v update-desktop-database >/dev/null; then update-desktop-database "$data/applications" || true; fi
  if command -v gtk-update-icon-cache >/dev/null; then
    gtk-update-icon-cache -f -t "$data/icons/hicolor" >/dev/null 2>&1 || true
  fi
  say "Installed the Codync desktop app to $BIN_DIR/codync"
  if command -v xdg-mime >/dev/null; then
    xdg-mime default codync.desktop x-scheme-handler/codync || true
    xdg-mime default codync.desktop x-scheme-handler/com.pokai.codync || true
  fi
  pin_gnome_launcher
}

install_linux_dependencies() {
  missing=""
  add_missing() {
    case " $missing " in
      *" $1 "*) ;;
      *) missing="$missing $1" ;;
    esac
  }
  if [ $HOST_ONLY = 0 ] && command -v ldconfig >/dev/null && ! ldconfig -p 2>/dev/null | grep -q libfuse.so.2; then
    if command -v apt-cache >/dev/null && apt-cache show libfuse2t64 >/dev/null 2>&1; then
      add_missing libfuse2t64
    else
      add_missing libfuse2
    fi
  fi
  if ldd "$BIN_DIR/codync-screen" 2>/dev/null | grep -q 'libgstwebrtc-1.0.so.0 => not found'; then
    add_missing libgstreamer-plugins-bad1.0-0
    add_missing gstreamer1.0-plugins-bad
  fi
  if [ $HOST_ONLY = 0 ] && [ -n "${WAYLAND_DISPLAY:-}" ] &&
    { ! command -v gst-inspect-1.0 >/dev/null || ! gst-inspect-1.0 pipewiresrc >/dev/null 2>&1; }; then
      add_missing gstreamer1.0-pipewire
  fi
  if [ -n "$missing" ] && command -v apt-get >/dev/null; then
    if [ "$(id -u)" = 0 ]; then
      say "Installing runtime dependencies:$missing"
      DEBIAN_FRONTEND=noninteractive apt-get install -y $missing || say "Some runtime dependencies couldn't be installed"
    elif [ -t 0 ] || { [ -r /dev/tty ] && [ -w /dev/tty ]; }; then
      say "Installing runtime dependencies:$missing"
      DEBIAN_FRONTEND=noninteractive sudo apt-get install -y $missing </dev/tty || say "Some runtime dependencies couldn't be installed"
    else
      echo "   Install runtime dependencies with: sudo apt-get install$missing"
    fi
  elif [ -n "$missing" ]; then
    echo "   Install runtime dependencies with your package manager:$missing"
  fi
}

ensure_host_service() {
  unit=${XDG_CONFIG_HOME:-$HOME/.config}/systemd/user/codync-host.service
  if [ -f "$unit" ] || systemctl --user cat codync-host.service >/dev/null 2>&1; then
    # Keep an existing unit and its custom port, but finish an interrupted install.
    if ! systemctl --user is-enabled codync-host.service >/dev/null 2>&1 &&
      ! systemctl --user enable --now codync-host.service; then
      service_install_failed=1
    fi
  elif ! "$BIN_DIR/codync-host" install; then
    service_install_failed=1
  fi
}

if [ $OS = macos ] && [ $HOST_ONLY = 0 ]; then
  install_mac_app
  echo
  echo "Codync is open and sets up the host on its own. Pair your phone from the menu bar: Pair phone…"
else
  install_host
  if [ $OS = linux ] && [ $HOST_ONLY = 0 ] && { [ $FORCE_APP = 1 ] || [ -n "${WAYLAND_DISPLAY:-}${DISPLAY:-}" ]; }; then
    install_linux_app
  fi
  if [ $OS = linux ]; then
    install_linux_dependencies
    [ $HOST_ONLY = 1 ] || ensure_host_service
  fi
  echo
  if [ $HOST_ONLY = 0 ]; then
    echo "Next: open Codync, then choose Pair phone"
  else
    echo "Next: run $BIN_DIR/codync-host install, then $BIN_DIR/codync-host pair"
  fi
fi

if [ "$OS" = linux ] && [ "$BIN_DIR" = "$HOME/.local/bin" ]; then
  add_path='case ":$PATH:" in *:"$HOME/.local/bin":*) ;; *) export PATH="$HOME/.local/bin:$PATH" ;; esac'
  changed=0
  for file in "$HOME/.profile" "$HOME/.bashrc"; do
    touch "$file"
    # An existing entry (including Ubuntu's default .profile block) already does the job.
    if ! grep -Fq "$BIN_DIR" "$file" &&
      ! grep -Fq '$HOME/.local/bin' "$file" &&
      ! grep -Fq '${HOME}/.local/bin' "$file"; then
      printf '\n# Codync\n%s\n' "$add_path" >> "$file"
      changed=1
    fi
  done
  case :$PATH: in
    *:"$BIN_DIR":*) ;;
    *) echo; echo "Open a new terminal to use codync-host." ;;
  esac
  [ "$changed" = 0 ] || echo "Added $BIN_DIR to new login and interactive Bash shells."
  echo "Open a new terminal, or run $BIN_DIR/codync-host pair."
else
  case :$PATH: in
    *:"$BIN_DIR":*) ;;
    *) echo; echo "Add $BIN_DIR to your PATH: export PATH=\"$BIN_DIR:\$PATH\"" ;;
  esac
fi

if [ $OS = linux ] && [ -n "${service_install_failed:-}" ]; then
  echo "codync-host is installed, but its background service could not start in this session."
  echo "Run $BIN_DIR/codync-host install from a signed-in desktop session."
  exit 0
fi
