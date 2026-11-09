// A cloud computer's first-boot script on dev and test clouds: the host build and service environment.

import { describe, expect, it } from "vitest";
import { hostEnv, machineName, setupScript } from "../src/cloud-setup";
import type { Env } from "../src/index";

const env = (over: Partial<Env>) => over as Env;

describe("cloud computer setup", () => {
  it("installs the release, then a test build and the service environment when the cloud names them", () => {
    const plain = setupScript(env({})).trimEnd().split("\n");
    expect(plain.slice(0, 3)).toEqual([
      "set -eu",
      "curl -fsSL https://raw.githubusercontent.com/leepokai/Codync/main/packaging/install.sh | sh -s -- --host-only",
      'export PATH="$HOME/.local/bin:$PATH"',
    ]);
    const script = setupScript(
      env({ CLOUD_HOST_BINARY_URL: "http://127.0.0.1:5000/codync-host", CLOUD_HOST_ENV: "CODYNC_CLOUD_URL=http://127.0.0.1:4000 CODYNC_ANALYTICS_ENV=dev" }),
    );
    expect(script).toContain("curl -fsSL --retry 60 --retry-delay 2 --retry-connrefused 'http://127.0.0.1:5000/codync-host' -o ~/.local/bin/codync-host.next");
    expect(script).toContain(
      "printf '%s\\n' '[Service]' 'Environment=CODYNC_CLOUD_URL=http://127.0.0.1:4000' 'Environment=CODYNC_ANALYTICS_ENV=dev' | sudo -n -u codync-agent tee \"$AH/.config/systemd/user/codync-host.service.d/codync-cloud.conf\" >/dev/null",
    );
    // The keeper starts with the same environment (it fetches its vault key from that cloud).
    expect(script).toContain("'Environment=CODYNC_CLOUD_URL=http://127.0.0.1:4000'");
  });

  it("puts the keeper under its own user, and the host and its agents under one that never had sudo", () => {
    const script = setupScript(env({ CLOUD_HOST_ENV: "CODYNC_CLOUD_URL=http://127.0.0.1:4000" }));
    const lines = script.trimEnd().split("\n");
    const at = (needle: string) => lines.findIndex((l) => l.includes(needle));
    expect(lines[at("useradd --system")]).toContain(" codync");
    expect(lines[at("codync-keeper/codync-host keeper")]).toContain("'User=codync'");
    expect(lines[at("codync-keeper/codync-host keeper")]).toContain("'StateDirectoryMode=0700'");
    expect(at("enable --now codync-keeper")).toBeGreaterThan(at("install -o codync -g codync -m 0700"));
    // The agents' user: no group but its own, the claim ticket moved into its home, the host installed as it.
    expect(lines[at("useradd --create-home")]).toMatch(/--user-group codync-agent$/);
    expect(script).not.toMatch(/usermod|gpasswd|-G |--groups/);
    expect(at('"$AH/.codync/managed.json"')).toBeGreaterThan(at("useradd --create-home"));
    expect(lines[at("codync-cloud.conf")]).toContain("Environment=CODYNC_CLOUD_URL=http://127.0.0.1:4000");
    expect(lines[at("codync-cloud.conf")]).toContain("sudo -n -u codync-agent tee");
    expect(at("enable-linger codync-agent")).toBeGreaterThan(at("enable --now codync-keeper"));
    expect(lines.at(-2)).toMatch(/^sudo -n -u codync-agent env HOME="\$AH" .* "\$AH\/\.local\/bin\/codync-host" install$/);
    // Agents the image installs on first run as root are installed now, last.
    expect(lines.at(-1)).toContain('"$L" --install "$h"');
    // The provider's own user keeps its sudo (its machine agent may need it): nothing here takes it away.
    expect(script).not.toMatch(/sudoers|restart "user@/);
  });

  it("refuses anything but plain CODYNC_ settings and URLs", () => {
    expect(hostEnv(" CODYNC_SLEEP_IDLE_SECS=20 ")).toEqual([["CODYNC_SLEEP_IDLE_SECS", "20"]]);
    expect(() => hostEnv("PATH=/tmp")).toThrow();
    expect(() => hostEnv("CODYNC_CLOUD_URL=x';rm")).toThrow();
    expect(() => setupScript(env({ CLOUD_HOST_BINARY_URL: "http://a/b';rm -rf ~" }))).toThrow();
    expect(machineName(env({ CLOUD_MACHINE_PREFIX: "codync-e2e" }), "user_2abcdefghiJKLMnop", "cm_xY_z-9Q")).toBe("codync-e2e-ijklmnop-y0z-9q");
  });
});
