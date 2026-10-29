// The cron that runs the release gate while a release waits behind GitHub's latest mark or the tap trails it.

import { afterEach, describe, expect, it, vi } from "vitest";
import type { Env } from "../src/index";
import { isNewer, nudgeReleaseGate, type Release, type State } from "../src/release-gate";

/** Stubs fetch to answer the GraphQL state query with `releases`, the `latest` mark and the tap's cask (at
 * `tap`, by default the latest release); returns the dispatch requests sent. */
function fake(releases: Release[], latest: string, { dispatch = 204, tap = `  version "${latest.slice(1)}"` } = {}) {
  const state: State = {
    codync: { latestRelease: { tagName: latest }, releases: { nodes: releases } },
    tap: { cask: { text: `cask "codync" do\n${tap}\nend\n` } },
  };
  const spy = vi.spyOn(globalThis, "fetch").mockImplementation(async (input) =>
    String(input).endsWith("/graphql") ? Response.json({ data: state }) : new Response(null, { status: dispatch }),
  );
  return () =>
    spy.mock.calls.filter(([url]) => String(url).endsWith("/dispatches")).map(([url, init]) => ({ url: String(url), init }));
}

const env = { RELEASE_GATE_TOKEN: "t" } as Env;

afterEach(() => vi.restoreAllMocks());

describe("release gate nudge", () => {
  it("compares versions", () => {
    expect(isNewer("v2.20.1", "v2.20.0")).toBe(true);
    expect(isNewer("v2.20.1", "v2.20.2")).toBe(false);
    expect(isNewer("v2.20.1", "v2.20.1")).toBe(false);
    expect(isNewer("v2.10.0", "v2.9.9")).toBe(true);
    expect(isNewer("garbage", "v2.20.0")).toBe(false);
    expect(isNewer("v2.21.0-rc.1", "v2.20.2")).toBe(false);
    expect(isNewer("v2.21", "v2.20.2")).toBe(false);
    expect(isNewer("v3.x", "v2.20.2")).toBe(false);
  });

  it("runs the gate while a release waits behind the latest mark", async () => {
    const dispatches = fake([{ tagName: "v2.20.2" }, { tagName: "v2.20.1" }, { tagName: "v2.20.0" }], "v2.20.0");
    await nudgeReleaseGate(env);
    const [call] = dispatches();
    expect(dispatches()).toHaveLength(1);
    expect(call.url).toBe("https://api.github.com/repos/leepokai/Codync/actions/workflows/release-gate.yml/dispatches");
    expect(JSON.parse(call.init?.body as string)).toEqual({ ref: "main" });
    expect((call.init?.headers as Record<string, string>).Authorization).toBe("Bearer t");
  });

  it("leaves the gate alone when the latest mark is on the newest release", async () => {
    const dispatches = fake([{ tagName: "v2.20.2" }, { tagName: "v2.20.1" }], "v2.20.2");
    await nudgeReleaseGate(env);
    expect(dispatches()).toHaveLength(0);
  });

  it("runs the gate while the Homebrew tap trails the latest release", async () => {
    const dispatches = fake([{ tagName: "v2.20.2" }], "v2.20.2", { tap: `  version "2.20.1"` });
    await nudgeReleaseGate(env);
    expect(dispatches()).toHaveLength(1);
  });

  it("fails loudly on a cask it can't read the version of", async () => {
    fake([{ tagName: "v2.20.2" }], "v2.20.2", { tap: "" });
    await expect(nudgeReleaseGate(env)).rejects.toThrow("no version line");
  });

  it("ignores drafts and pre-releases", async () => {
    const dispatches = fake([{ tagName: "v2.21.0", isDraft: true }, { tagName: "v2.20.3", isPrerelease: true }, { tagName: "v2.20.2" }], "v2.20.2");
    await nudgeReleaseGate(env);
    expect(dispatches()).toHaveLength(0);
  });

  it("does nothing without a token", async () => {
    const dispatches = fake([{ tagName: "v2.20.2" }], "v2.20.0");
    await nudgeReleaseGate({} as Env);
    expect(dispatches()).toHaveLength(0);
  });

  it("fails loudly when GitHub refuses the dispatch", async () => {
    fake([{ tagName: "v2.20.2" }], "v2.20.0", { dispatch: 403 });
    await expect(nudgeReleaseGate(env)).rejects.toThrow("release gate dispatch: 403");
  });
});
