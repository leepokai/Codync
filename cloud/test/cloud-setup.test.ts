// A cloud computer's first-boot script on dev and test clouds: the host build and service environment.

import { describe, expect, it } from "vitest";
import { hostEnv, machineName, setupScript } from "../src/cloud-setup";
import type { Env } from "../src/index";

const env = (over: Partial<Env>) => over as Env;

describe("cloud computer setup", () => {
  it("installs the release, then a test build and the service environment when the cloud names them", () => {
    expect(setupScript(env({}))).toBe(
      "set -eu\ncurl -fsSL https://raw.githubusercontent.com/leepokai/Codync/main/packaging/install.sh | sh -s -- --host-only\n" +
        'export PATH="$HOME/.local/bin:$PATH"\ncodync-host install\n',
    );
    const script = setupScript(
      env({ CLOUD_HOST_BINARY_URL: "http://127.0.0.1:5000/codync-host", CLOUD_HOST_ENV: "CODYNC_CLOUD_URL=http://127.0.0.1:4000 CODYNC_ANALYTICS_ENV=dev" }),
    );
    expect(script).toContain("curl -fsSL --retry 60 --retry-delay 2 --retry-connrefused 'http://127.0.0.1:5000/codync-host' -o ~/.local/bin/codync-host.next");
    expect(script).toContain(
      "printf '[Service]\\nEnvironment=CODYNC_CLOUD_URL=http://127.0.0.1:4000\\nEnvironment=CODYNC_ANALYTICS_ENV=dev\\n' > ~/.config/systemd/user/codync-host.service.d/codync-cloud.conf",
    );
    expect(script.trimEnd().endsWith("codync-host install")).toBe(true);
  });

  it("refuses anything but plain CODYNC_ settings and URLs", () => {
    expect(hostEnv(" CODYNC_SLEEP_IDLE_SECS=20 ")).toEqual([["CODYNC_SLEEP_IDLE_SECS", "20"]]);
    expect(() => hostEnv("PATH=/tmp")).toThrow();
    expect(() => hostEnv("CODYNC_CLOUD_URL=x';rm")).toThrow();
    expect(() => setupScript(env({ CLOUD_HOST_BINARY_URL: "http://a/b';rm -rf ~" }))).toThrow();
    expect(machineName(env({ CLOUD_MACHINE_PREFIX: "codync-e2e" }), "user_2abcdefghijklmnop")).toBe("codync-e2e-efghijklmnop");
  });
});
