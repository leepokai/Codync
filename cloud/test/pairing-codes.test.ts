// Short pairing codes (docs/plans/require-sign-in.md, phase 3).

import { describe, expect, it } from "vitest";
import { normalizeCode } from "../src/routes/pairing-codes";
import { call, clerkToken, newHost } from "./helpers";

const LINK = "codync-dev://pair?v=3&name=VPS&id=abc&code=xyz";

describe("pairing codes", () => {
  it("trades a code a computer registered for its link, once", async () => {
    const host = await newHost();
    const created = await call("POST", "/v1/host/pairing-codes", { key: host.sign, body: { link: LINK } });
    expect(created.status).toBe(200);
    const code = created.body.code as string;
    expect(code).toMatch(/^[A-HJ-NP-Z]{4}-[0-9]{4}$/);
    const token = await clerkToken(`user_codes_${Date.now()}`);
    // Typed loosely: lower case, no dash.
    const typed = code.toLowerCase().replace("-", "");
    const redeemed = await call("GET", `/v1/pairing-codes/${typed}`, { token });
    expect(redeemed).toMatchObject({ status: 200, body: { link: LINK } });
    expect((await call("GET", `/v1/pairing-codes/${code}`, { token })).status).toBe(404);
  });

  it("needs a signed-in app, a registered computer and a pairing link", async () => {
    const host = await newHost();
    const created = await call("POST", "/v1/host/pairing-codes", { key: host.sign, body: { link: LINK } });
    expect((await call("GET", `/v1/pairing-codes/${created.body.code}`)).status).toBe(401);
    expect((await call("POST", "/v1/host/pairing-codes", { key: host.sign, body: { link: "https://evil.example" } })).status).toBe(400);
    const token = await clerkToken(`user_codes_${Date.now()}`);
    expect((await call("GET", "/v1/pairing-codes/NOPE-0000", { token })).status).toBe(404);
  });

  it("limits guessing per account, from any address", async () => {
    const token = await clerkToken(`user_guess_${Date.now()}`);
    const tries: number[] = [];
    // Misses count twice against the account's ten a minute (each call comes from another IP).
    while (tries.length < 12 && tries.at(-1) !== 429) tries.push((await call("GET", `/v1/pairing-codes/GUES-${1000 + tries.length}`, { token })).status);
    expect(tries.at(-1)).toBe(429);
    expect(tries.slice(0, -1).every((status) => status === 404)).toBe(true);
  });

  it("normalizes what people type", () => {
    expect(normalizeCode(" qrtx 4821 ")).toBe("QRTX-4821");
    expect(normalizeCode("QRTX-48210")).toBeNull();
  });
});
