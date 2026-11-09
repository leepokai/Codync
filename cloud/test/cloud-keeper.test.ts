// Codync Cloud's keeper (docs/plans/codync-cloud.md, "Agents and the host's secrets"): the vault key a machine gets at
// create goes only to the keeper its claim named, and is gone with the machine.

import { afterEach, describe, expect, it, vi } from "vitest";
import { boat, cc, hostClaim, uid, withKey } from "./cloud-helpers";
import { call, env, newHost } from "./helpers";
import * as ref from "./ref";

afterEach(() => vi.restoreAllMocks());

const vaultKey = (key: ref.SignKey) => call("GET", "/v1/keeper/vault-key", { key });

describe("a cloud computer's keeper", () => {
  it("gets the vault key only with the key its claim named, and never once the machine is deleted", async () => {
    const userId = uid();
    await withKey(userId);
    const fetch = boat();
    const created = await cc("POST", userId, "/v1/cloud-computers", { provider: "boat" });
    const id = created.body.cloudComputer.cloudComputerId as string;
    const row = await env.DB.prepare("SELECT vault_sealed, keeper_key FROM cloud_machines WHERE id = ?").bind(id).first<Record<string, string | null>>();
    expect(row).toMatchObject({ vault_sealed: expect.any(String), keeper_key: null });
    // The sealed key is in D1, never in what the provider gets.
    const [, init] = fetch.mock.calls.find(([url, i]) => i?.method === "POST" && String(url).endsWith("/sandboxes"))!;
    expect(String(init!.body)).not.toContain(row!.vault_sealed!);

    const keeper = ref.signKey();
    // No claim has named it yet.
    expect((await vaultKey(keeper)).status).toBe(404);

    const ticket = JSON.parse(JSON.parse(init!.body as string).files[0].content);
    const host = await newHost();
    expect((await hostClaim(host, ticket, { keeperKey: "short" })).status).toBe(400);
    expect((await hostClaim(host, ticket, { keeperKey: ref.b64url(keeper.pub) })).status).toBe(200);

    const got = await vaultKey(keeper);
    expect(got.status).toBe(200);
    expect(ref.unb64url(got.body.key as string)).toHaveLength(32);
    expect((await vaultKey(keeper)).body.key).toBe(got.body.key);
    // The host's own key (which its agents could read) and any other key get nothing.
    expect((await vaultKey(host.sign)).status).toBe(404);
    expect((await vaultKey(ref.signKey())).status).toBe(404);

    boat("cancelled");
    expect((await cc("DELETE", userId, `/v1/cloud-computers/${id}`)).status).toBe(200);
    expect((await vaultKey(keeper)).status).toBe(404);
    const gone = await env.DB.prepare("SELECT vault_sealed FROM cloud_machines WHERE id = ?").bind(id).first<{ vault_sealed: string | null }>();
    expect(gone!.vault_sealed).toBeNull();
  });
});
