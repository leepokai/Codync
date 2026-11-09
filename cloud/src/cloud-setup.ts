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

/** Makes the agents' user, hands it the binary and the claim ticket the provider left in its own user's home, starts
 * its user manager (linger) and installs the host's service there, with `vars` in its environment. */
function agentLines(vars: [string, string][]): string[] {
  const as = `sudo -n -u ${AGENT} env HOME="$AH" XDG_RUNTIME_DIR="/run/user/$AU" DBUS_SESSION_BUS_ADDRESS="unix:path=/run/user/$AU/bus"`;
  return [
    `AH="$(dirname "$HOME")/${AGENT}"`,
    `id -u ${AGENT} >/dev/null 2>&1 || sudo -n useradd --create-home --home-dir "$AH" --shell /bin/bash --user-group ${AGENT}`,
    `AU="$(id -u ${AGENT})"`,
    `sudo -n install -d -o ${AGENT} -g ${AGENT} -m 0700 "$AH/.codync"`,
    `sudo -n install -d -o ${AGENT} -g ${AGENT} -m 0755 "$AH/.local" "$AH/.local/bin"`,
    `sudo -n install -o ${AGENT} -g ${AGENT} -m 0755 "$HOME/.local/bin/codync-host" "$AH/.local/bin/codync-host"`,
    `sudo -n install -o ${AGENT} -g ${AGENT} -m 0600 "$HOME/.codync/managed.json" "$AH/.codync/managed.json"`,
    "rm -f ~/.codync/managed.json",
    `sudo -n install -d -o ${AGENT} -g ${AGENT} -m 0755 "$AH/.config" "$AH/.config/systemd" "$AH/.config/systemd/user" "$AH/.config/systemd/user/codync-host.service.d"`,
    writeFile('"$AH/.config/systemd/user/codync-host.service.d/codync-cloud.conf"', ["[Service]", ...envLines(vars)], AGENT),
    `sudo -n loginctl enable-linger ${AGENT}`,
    'for _ in $(seq 60); do [ -S "/run/user/$AU/bus" ] && break; sleep 1; done',
    `${as} "$AH/.local/bin/codync-host" install`,
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
  lines.push(...keeperLines(vars), ...agentLines(vars), PROVIDER_AGENTS);
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
