// The relay and public webhooks are only for computers in an account (docs/plans/optional-sign-in.md).

import { describe, expect, it } from "vitest";
import { mayUseRelay, relayNeedsAccount, routineHook } from "../src/routes/relay";
import type { Ctx } from "../src/routes/common";
import { env, newHost, ORIGIN } from "./helpers";

async function account(id: string) {
  await env.DB.prepare("INSERT OR IGNORE INTO accounts(user_id, created_at) VALUES (?, 0)").bind(id).run();
}

async function device(id: string, owner: string, key: string, o: { revoked?: boolean; signedOut?: boolean } = {}) {
  await account(owner);
  await env.DB.prepare(
    `INSERT INTO devices(id, owner_user_id, sign_pub, name, platform, created_at, revoked_at, signed_out_at)
     VALUES (?, ?, ?, 'Phone', 'ios', 0, ?, ?)`,
  )
    .bind(id, owner, key, o.revoked ? 1 : null, o.signedOut ? 1 : null)
    .run();
}

describe("mayUseRelay", () => {
  it("lets in only a device signed in to the computer's account", async () => {
    await device("dev_a", "user_a", "key_a");
    await device("dev_b", "user_b", "key_b");
    await device("dev_gone", "user_a", "key_gone", { revoked: true });
    await device("dev_out", "user_a", "key_out", { signedOut: true });

    expect(await mayUseRelay(env.DB, "user_a", "key_a", false)).toBe(true);
    expect(await mayUseRelay(env.DB, "user_a", "key_b", false), "another account's phone").toBe(false);
    expect(await mayUseRelay(env.DB, "user_a", "key_none", false), "a phone with no account").toBe(false);
    expect(await mayUseRelay(env.DB, "user_a", "key_gone", false), "a revoked phone").toBe(false);
    expect(await mayUseRelay(env.DB, "user_a", "key_out", false), "a signed-out phone").toBe(false);
    expect(await mayUseRelay(env.DB, null, "key_a", false), "a computer in no account").toBe(false);
  });

  it("lets a signed-in phone pair through it, so a computer with no other route can join its account", async () => {
    await device("dev_p", "user_p", "key_p");
    expect(await mayUseRelay(env.DB, null, "key_p", true)).toBe(true);
    expect(await mayUseRelay(env.DB, "user_other", "key_p", true), "the computer turns it away itself").toBe(true);
    expect(await mayUseRelay(env.DB, null, "key_stranger", true), "a phone with no account").toBe(false);
  });
});

describe("the switch", () => {
  it("is off unless set", () => {
    expect(relayNeedsAccount(env)).toBe(false);
    expect(relayNeedsAccount({ ...env, RELAY_NEEDS_ACCOUNT: "1" })).toBe(true);
  });

  it("refuses a public webhook for a computer in no account", async () => {
    const host = await newHost();
    const url = new URL(`${ORIGIN}/v1/hooks/${host.cid}/rtn_1`);
    const ctx = {
      env: { ...env, RELAY_NEEDS_ACCOUNT: "1" },
      req: new Request(url, { method: "POST" }),
      raw: new Uint8Array(),
      url,
      now: Date.now(),
    } as unknown as Ctx;
    await expect(routineHook(ctx, [host.cid, "rtn_1"])).rejects.toMatchObject({ code: "needsAccount", status: 403 });
  });
});
