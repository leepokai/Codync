// What a new cloud computer runs once, when it is first ready: the public host install, then, on dev and test
// clouds only, a host build under test and the environment its service starts with
// (docs/guides/cloud-computer-testing.md).

import type { Env } from "./index";

const INSTALL = "https://raw.githubusercontent.com/leepokai/Codync/main/packaging/install.sh";
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

export function setupScript(env: Env): string {
  const lines = ["set -eu", `curl -fsSL ${INSTALL} | sh -s -- --host-only`, 'export PATH="$HOME/.local/bin:$PATH"'];
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
  const vars = hostEnv(env.CLOUD_HOST_ENV);
  if (vars.length) {
    const unit = ["[Service]", ...vars.map(([k, v]) => `Environment=${k}=${v}`)].join("\\n");
    lines.push(
      "mkdir -p ~/.config/systemd/user/codync-host.service.d",
      `printf '${unit}\\n' > ~/.config/systemd/user/codync-host.service.d/codync-cloud.conf`,
    );
  }
  lines.push("codync-host install");
  return `${lines.join("\n")}\n`;
}

/** The provider's name for a user's machine; test clouds use their own prefix so leftovers are easy to find. */
export const machineName = (env: Env, userId: string) => `${env.CLOUD_MACHINE_PREFIX ?? "codync"}-${userId.slice(-12)}`;
