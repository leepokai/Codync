// What a new cloud computer runs once, when it is first ready, as the provider's own user (which has passwordless sudo,
// and keeps it: the provider's machine agent may need it, Boat's snapshots do): the host install from this cloud's
// release channel (`host-releases.ts`; on test clouds, a host build under test and its service's environment:
// docs/guides/cloud-computer-testing.md); the keeper, the same binary under its own system user `codync`, which holds
// the secrets; and the user Codync's host and agents run as, `codync-agent`, which never has sudo or any group that is
// root by another name, so its agents can't reach them (docs/plans/codync-cloud.md, "Agents and the host's secrets").

import type { Setup } from "../machines/src";
import { releasesUrl } from "./host-releases";
import type { Env } from "./index";

const URL = /^https?:\/\/[A-Za-z0-9.:[\]_/-]+$/;
const VAR = /^(CODYNC_[A-Z_]{1,40})=([A-Za-z0-9:/._-]{1,200})$/;

/** `CLOUD_HOST_ENV`: `CODYNC_…=value` pairs separated by spaces, for the host's service. */
export function hostEnv(raw: string | undefined): [string, string][] {
  return (raw ?? "")
    .split(/\s+/)
    .filter(Boolean)
    .map((pair): [string, string] => {
      const m = VAR.exec(pair);
      if (!m) throw new Error(`Invalid CLOUD_HOST_ENV entry: ${pair}`);
      return [m[1]!, m[2]!];
    });
}

/** `Environment=` lines for a systemd unit. */
const envLines = (vars: [string, string][]) => vars.map(([k, v]) => `Environment=${k}=${v}`);

/** Writes `lines` to `path` as root, or as user `as`: each line is its own `printf` argument, never its format. Every
 * line here is fixed, a checked setting (`VAR`) or the checked release channel (`CHANNEL` in `host-releases.ts`), so
 * none holds a `'`. */
const writeFile = (path: string, lines: string[], as?: string) =>
  `printf '%s\\n' ${lines.map((l) => `'${l}'`).join(" ")} | sudo -n ${as ? `-u ${as} ` : ""}tee ${path} >/dev/null`;

/** Where the keeper lives: its state, and its own copy of the binary, which only it may replace (`update`). */
const KEEPER_DIR = "/var/lib/codync-keeper";

/** The keeper's system unit: its own user, its own state folder (0700), its socket under /run. */
function keeperUnit(vars: [string, string][]): string[] {
  return [
    "[Unit]",
    "Description=Codync keeper",
    "Wants=network-online.target",
    "After=network-online.target",
    "[Service]",
    "User=codync",
    "Group=codync",
    `ExecStart=${KEEPER_DIR}/codync-host keeper`,
    "StateDirectory=codync-keeper",
    "StateDirectoryMode=0700",
    "RuntimeDirectory=codync-keeper",
    "RuntimeDirectoryMode=0755",
    ...envLines(vars),
    "Restart=always",
    "RestartSec=2",
    "NoNewPrivileges=yes",
    "ProtectSystem=strict",
    "ProtectHome=yes",
    "PrivateTmp=yes",
    "[Install]",
    "WantedBy=multi-user.target",
  ];
}

/** The keeper's user, its unit, and the setting that keeps processes of one user from attaching to each other where
 * the kernel has Yama (the host also makes itself non-dumpable). */
function keeperLines(vars: [string, string][]): string[] {
  return [
    `id -u codync >/dev/null 2>&1 || sudo -n useradd --system --user-group --no-create-home --home-dir ${KEEPER_DIR} --shell /usr/sbin/nologin codync`,
    `sudo -n install -d -o codync -g codync -m 0700 ${KEEPER_DIR}`,
    `sudo -n install -o codync -g codync -m 0700 "$HOME/.local/bin/codync-host" ${KEEPER_DIR}/codync-host`,
    writeFile("/etc/systemd/system/codync-keeper.service", keeperUnit(vars)),
    "sudo -n systemctl daemon-reload",
    "sudo -n systemctl enable --now codync-keeper",
    writeFile("/etc/sysctl.d/60-codync.conf", ["kernel.yama.ptrace_scope = 1"]),
    "sudo -n sysctl -q -p /etc/sysctl.d/60-codync.conf || true",
  ];
}

/** The user Codync's host and its agents run as: made here, never in a sudo or admin group, its home beside the
 * provider user's (on the disk the provider keeps). */
const AGENT = "codync-agent";

/** Runs a command as the agents' user, in its user manager's session (after `agentLines` set `AH` and `AU`). */
const asAgent = `sudo -n -u ${AGENT} env HOME="$AH" XDG_RUNTIME_DIR="/run/user/$AU" DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$AU/bus"`;

/** The display of the desktop the computer's screen shows (`desktopLines`). */
const DESKTOP_DISPLAY = ":20";

/** Makes the agents' user, hands it the binaries (the host, and beside it the screen helper and the computer-use
 * driver it starts) and the claim ticket the provider left in its own user's home, starts its user manager (linger)
 * and installs the host's service there, with `vars` and the desktop's display in its environment. */
function agentLines(vars: [string, string][]): string[] {
  return [
    `AH="$(dirname "$HOME")/${AGENT}"`,
    `id -u ${AGENT} >/dev/null 2>&1 || sudo -n useradd --create-home --home-dir "$AH" --shell /bin/bash --user-group ${AGENT}`,
    `AU="$(id -u ${AGENT})"`,
    `sudo -n install -d -o ${AGENT} -g ${AGENT} -m 0700 "$AH/.codync"`,
    `sudo -n install -d -o ${AGENT} -g ${AGENT} -m 0755 "$AH/.local" "$AH/.local/bin"`,
    `sudo -n install -o ${AGENT} -g ${AGENT} -m 0755 "$HOME/.local/bin/codync-host" "$AH/.local/bin/codync-host"`,
    `for b in codync-screen cua-driver; do if [ -f "$HOME/.local/bin/$b" ]; then sudo -n install -o ${AGENT} -g ${AGENT} -m 0755 "$HOME/.local/bin/$b" "$AH/.local/bin/$b"; fi; done`,
    `sudo -n install -o ${AGENT} -g ${AGENT} -m 0600 "$HOME/.codync/managed.json" "$AH/.codync/managed.json"`,
    "rm -f ~/.codync/managed.json",
    `sudo -n install -d -o ${AGENT} -g ${AGENT} -m 0755 "$AH/.config" "$AH/.config/systemd" "$AH/.config/systemd/user" "$AH/.config/systemd/user/codync-host.service.d"`,
    writeFile('"$AH/.config/systemd/user/codync-host.service.d/codync-cloud.conf"', ["[Service]", ...envLines([...vars, ["DISPLAY", DESKTOP_DISPLAY]])], AGENT),
    `sudo -n loginctl enable-linger ${AGENT}`,
    'for _ in $(seq 60); do [ -S "/run/user/$AU/bus" ] && break; sleep 1; done',
    `${asAgent} "$AH/.local/bin/codync-host" install`,
  ];
}

/** What the desktop needs: a virtual display, an Xfce session, GStreamer with an H.264 encoder (x264) and ICE
 * (libnice) for the screen helper, and what it uses for the clipboard (xclip), reading windows for bots (AT-SPI) and
 * opening apps (gtk-launch). InstaBox's desktop image already has the display and Xfce. */
const DESKTOP_PACKAGES = [
  "xvfb",
  "dbus-x11",
  "xfce4-session",
  "xfwm4",
  "xfce4-panel",
  "xfdesktop4",
  "xfce4-terminal",
  "gstreamer1.0-plugins-base",
  "gstreamer1.0-plugins-good",
  "gstreamer1.0-plugins-bad",
  "gstreamer1.0-plugins-ugly",
  "gstreamer1.0-nice",
  "xclip",
  "at-spi2-core",
  "libgtk-3-bin",
];

/** The agents' virtual display, at the size phones and the desktop app see. */
const displayUnit = [
  "[Unit]",
  "Description=Codync desktop display",
  "[Service]",
  // `-noreset`: a display that resets whenever its last client leaves would drop the helper with it.
  `ExecStart=/usr/bin/Xvfb ${DESKTOP_DISPLAY} -screen 0 1920x1080x24 -nolisten tcp -noreset`,
  "Restart=always",
  "RestartSec=2",
  "[Install]",
  "WantedBy=default.target",
];

/** The Xfce session on it, on the agents' user bus, where the screen helper and the computer-use driver find the apps'
 * accessibility bus (a private session bus would start another, empty one). That bus starts on demand with the user
 * manager's environment, so the display goes there first. */
const sessionUnit = [
  "[Unit]",
  "Description=Codync desktop session",
  "Requires=codync-display.service",
  "After=codync-display.service",
  "[Service]",
  ...envLines([["DISPLAY", DESKTOP_DISPLAY]]),
  `ExecStartPre=/usr/bin/systemctl --user set-environment DISPLAY=${DESKTOP_DISPLAY}`,
  "ExecStart=/usr/bin/startxfce4",
  "Restart=always",
  "RestartSec=2",
  "[Install]",
  "WantedBy=default.target",
];

/** The desktop the computer's screen shows (phones and the desktop app watch and control it; bots use it): a virtual
 * display with an Xfce session, run as the agents' user. Never the provider's own desktop (InstaBox's Xvnc, Boat's
 * Xorg): it runs as the provider's user, which has passwordless sudo, so handing it to the agents would hand them root.
 * Last, after the host is up: the packages take a minute, and the host's screen helper comes up once they're in. */
function desktopLines(): string[] {
  const unit = (name: string) => `"$AH/.config/systemd/user/${name}"`;
  // apt waits for its lock (first-boot updates may hold it) and gets a second try; without the packages the screen
  // stays off, which the setup log says.
  const apt = "sudo -n env DEBIAN_FRONTEND=noninteractive apt-get -qq -o DPkg::Lock::Timeout=300";
  return [
    `if command -v apt-get >/dev/null; then ok=; for _ in 1 2; do ${apt} update && ${apt} install -y --no-install-recommends ${DESKTOP_PACKAGES.join(" ")} >/dev/null && ok=1 && break; sleep 10; done; [ -n "$ok" ] || echo "Codync: the desktop packages didn't install, so this computer's screen stays off" >&2; fi`,
    writeFile(unit("codync-display.service"), displayUnit, AGENT),
    writeFile(unit("codync-desktop.service"), sessionUnit, AGENT),
    `${asAgent} systemctl --user daemon-reload`,
    `${asAgent} systemctl --user enable --now codync-display.service codync-desktop.service`,
  ];
}

/** Agents the image ships as wrappers that install themselves on their first run, as root (Boat's `lazy-run`:
 * Cursor, Hermes, OpenClaw, Mistral Vibe): installed now, while this user can, since the agents' user never will. Last,
 * so the host claims itself meanwhile; a failed one stays a wrapper. */
const PROVIDER_AGENTS =
  'L=/usr/local/lib/ascii-harnesses/lazy-run; if [ -x "$L" ]; then for h in cursor hermes openclaw vibe; do sudo -n "$L" --install "$h" >/dev/null 2>&1 || true; done; fi';

export function setupScript(env: Env): string {
  const releases = releasesUrl(env);
  const lines = [
    "set -eu",
    `curl -fsSL ${releases}/latest/download/install.sh | CODYNC_RELEASES_URL=${releases} sh -s -- --host-only`,
    'export PATH="$HOME/.local/bin:$PATH"',
  ];
  const binary = env.CLOUD_HOST_BINARY_URL;
  if (binary) {
    if (!URL.test(binary)) throw new Error("Invalid CLOUD_HOST_BINARY_URL");
    lines.push(
      // The test cloud's forward to this machine may come up a little after the machine does.
      `curl -fsSL --retry 60 --retry-delay 2 --retry-connrefused '${binary}' -o ~/.local/bin/codync-host.next`,
      "chmod 755 ~/.local/bin/codync-host.next",
      "mv ~/.local/bin/codync-host.next ~/.local/bin/codync-host",
    );
  }
  // The host and the keeper update from the same channel.
  const vars: [string, string][] = [["CODYNC_RELEASES_URL", releases], ...hostEnv(env.CLOUD_HOST_ENV)];
  lines.push(...keeperLines(vars), ...agentLines(vars), PROVIDER_AGENTS, ...desktopLines());
  return `${lines.join("\n")}\n`;
}

/** What a new cloud computer gets: its one-time claim ticket (the host claims itself with it on first boot) and the
 * setup script. */
export function machineSetup(env: Env, claimId: string, nonce: string, userId: string): Setup {
  const ticket = JSON.stringify({ claimId, nonce, userId, approval: "auto" });
  return { files: [{ path: ".codync/managed.json", content: ticket }], setupScript: setupScript(env) };
}

/** The provider's name for one of a user's machines (`machineId`: its `cm_…` id); test clouds use their own prefix so
 * leftovers are easy to find. */
export const machineName = (env: Env, userId: string, machineId: string) =>
  `${env.CLOUD_MACHINE_PREFIX ?? "codync"}-${userId.slice(-8)}-${machineId.slice(-6)}`.toLowerCase().replace(/[^a-z0-9-]/g, "0");
