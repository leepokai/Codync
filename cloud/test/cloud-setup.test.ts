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
      "curl -fsSL https://github.com/leepokai/Codync/releases/latest/download/install.sh | CODYNC_RELEASES_URL=https://github.com/leepokai/Codync/releases sh -s -- --host-only",
      'export PATH="$HOME/.local/bin:$PATH"',
    ]);
    const script = setupScript(
      env({ CLOUD_HOST_BINARY_URL: "http://127.0.0.1:5000/codync-host", CLOUD_HOST_ENV: "CODYNC_CLOUD_URL=http://127.0.0.1:4000 CODYNC_ANALYTICS_ENV=dev" }),
    );
    expect(script).toContain("curl -fsSL --retry 60 --retry-delay 2 --retry-connrefused 'http://127.0.0.1:5000/codync-host' -o ~/.local/bin/codync-host.next");
    expect(script).toContain(
      "printf '%s\\n' '[Service]' 'Environment=CODYNC_RELEASES_URL=https://github.com/leepokai/Codync/releases' 'Environment=CODYNC_CLOUD_URL=http://127.0.0.1:4000' 'Environment=CODYNC_ANALYTICS_ENV=dev' 'Environment=DISPLAY=:20' | sudo -n -u codync-agent tee \"$AH/.config/systemd/user/codync-host.service.d/codync-cloud.conf\" >/dev/null",
    );
    // The keeper starts with the same environment (it fetches its vault key from that cloud).
    expect(script).toContain("'Environment=CODYNC_CLOUD_URL=http://127.0.0.1:4000'");
  });

  it("installs from this cloud's release channel, and has the host and the keeper update from it", () => {
    const script = setupScript(env({ HOST_RELEASES_URL: "https://dev-releases.codync.dev" }));
    expect(script.split("\n")[1]).toBe(
      "curl -fsSL https://dev-releases.codync.dev/latest/download/install.sh | CODYNC_RELEASES_URL=https://dev-releases.codync.dev sh -s -- --host-only",
    );
    const keeper = script.split("\n").find((l) => l.includes("codync-keeper/codync-host keeper"));
    expect(keeper).toContain("'Environment=CODYNC_RELEASES_URL=https://dev-releases.codync.dev'");
    expect(script.split("\n").find((l) => l.includes("codync-cloud.conf") && l.includes("tee"))).toContain(
      "'Environment=CODYNC_RELEASES_URL=https://dev-releases.codync.dev'",
    );
    expect(() => setupScript(env({ HOST_RELEASES_URL: "https://dev-releases.codync.dev/;rm -rf ~" }))).toThrow();
    expect(() => setupScript(env({ HOST_RELEASES_URL: "http://dev-releases.codync.dev" }))).toThrow();
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
    const install = lines.findIndex((l) => /^sudo -n -u codync-agent env HOME="\$AH" .* "\$AH\/\.local\/bin\/codync-host" install$/.test(l));
    expect(install).toBeGreaterThan(at("enable-linger codync-agent"));
    // Agents the image installs on first run as root are installed after the host.
    expect(at('"$L" --install "$h"')).toBeGreaterThan(install);
    // The provider's own user keeps its sudo (its machine agent may need it): nothing here takes it away.
    expect(script).not.toMatch(/sudoers|restart "user@/);
  });

  it("gives the agents a desktop of their own for the screen, never the provider's", () => {
    const lines = setupScript(env({})).trimEnd().split("\n");
    const at = (needle: string) => lines.findIndex((l) => l.includes(needle));
    // The screen helper and the computer-use driver go beside the agents' host, which looks there.
    expect(lines[at("for b in codync-screen cua-driver")]).toContain('"$AH/.local/bin/$b"');
    // GStreamer with x264 and libnice, and the Xfce session, after the host is up.
    const packages = at("--no-install-recommends");
    expect(lines[packages]).toMatch(/xvfb .*xfce4-session .*gstreamer1\.0-plugins-ugly gstreamer1\.0-nice xclip/);
    expect(packages).toBeGreaterThan(at('"$AH/.local/bin/codync-host" install'));
    // A first-boot update holding apt's lock is waited for, and a failure is said, not swallowed.
    expect(lines[packages]).toContain("-o DPkg::Lock::Timeout=300");
    expect(lines[packages]).not.toContain("|| true");
    // The display and session are the agents' own user units; the host's environment names that display.
    expect(lines[at("codync-display.service\"")]).toMatch(/'ExecStart=\/usr\/bin\/Xvfb :20 -screen 0 1920x1080x24 -nolisten tcp -noreset' .*\| sudo -n -u codync-agent tee/);
    // The session uses the agents' user bus, whose accessibility bus the screen helper reads, and gives it the display.
    expect(lines[at("codync-desktop.service\"")]).toContain("'ExecStartPre=/usr/bin/systemctl --user set-environment DISPLAY=:20' 'ExecStart=/usr/bin/startxfce4'");
    expect(lines[at("codync-cloud.conf")]).toContain("'Environment=DISPLAY=:20'");
    expect(lines.at(-1)).toMatch(/^sudo -n -u codync-agent env .* systemctl --user enable --now codync-display\.service codync-desktop\.service$/);
    // Nothing opens the provider's own display (its user has sudo) to the agents.
    expect(lines.join("\n")).not.toMatch(/xhost|XAUTHORITY|DISPLAY=:[01]\b/);
  });

  it("refuses anything but plain CODYNC_ settings and URLs", () => {
    expect(hostEnv(" CODYNC_SLEEP_IDLE_SECS=20 ")).toEqual([["CODYNC_SLEEP_IDLE_SECS", "20"]]);
    expect(() => hostEnv("PATH=/tmp")).toThrow();
    expect(() => hostEnv("CODYNC_CLOUD_URL=x';rm")).toThrow();
    expect(() => setupScript(env({ CLOUD_HOST_BINARY_URL: "http://a/b';rm -rf ~" }))).toThrow();
    expect(machineName(env({ CLOUD_MACHINE_PREFIX: "codync-e2e" }), "user_2abcdefghiJKLMnop", "cm_xY_z-9Q")).toBe("codync-e2e-ijklmnop-y0z-9q");
  });
});
