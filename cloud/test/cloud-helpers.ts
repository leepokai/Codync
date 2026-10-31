// Codync Cloud test helpers: Boat stubbed with a fetch spy, the Worker with Boat configured, and a claimed
// cloud computer with its host connected.

import { expect, vi } from "vitest";
import worker from "../src/index";
import { ORIGIN, call, clerkToken, env, hostSocket, newHost, type Json, type TestHost } from "./helpers";
import * as ref from "./ref";

export const uid = () => `user_cc_${ref.b64url(ref.random(6))}`;

/** Calls `/v1/cloud-computers` (or `path`) with Boat configured and `userId` on the allowlist. */
export async function cc(method: string, userId: string, allowed = userId, path = "/v1/cloud-computers") {
  const res = await worker.fetch(
    new Request(`${ORIGIN}${path}`, { method, headers: { Authorization: `Bearer ${await clerkToken(userId)}` } }),
    { ...env, BOAT_API_KEY: "boat_test", CLOUD_COMPUTER_USERS: `someone_else, ${allowed}` },
    {} as ExecutionContext,
  );
  return { status: res.status, body: (await res.json()) as Json };
}

/** A fresh fetch spy (spying again returns the same spy, so earlier calls are cleared). */
export const spyFetch = () => vi.spyOn(globalThis, "fetch").mockClear();

/** Stubs Boat: every sandbox call answers `state`, and each request is recorded. */
export function boat(state = "provisioning") {
  return spyFetch().mockImplementation(async () => Response.json({ ok: true, sandbox: { id: "bx_test", state } }));
}

/** The host's side of the ticket: the claim signed with its key, sent to `/v1/host/claim`. */
export function hostClaim(host: TestHost, ticket: { claimId: string; nonce: string; userId: string }) {
  const boxKey = ref.b64url(host.box.pub);
  const canonical = ref.claimCanonical(ticket.claimId, ticket.nonce, ticket.userId, host.cid, boxKey);
  return call("POST", "/v1/host/claim", {
    key: host.sign,
    body: {
      claimId: ticket.claimId,
      userId: ticket.userId,
      computerId: host.cid,
      signKey: ref.b64url(host.sign.pub),
      boxKey,
      name: "codync-cloud",
      platform: "linux",
      version: "2.25.0",
      sig: ref.b64url(ref.sign(host.sign, ref.enc.encode(canonical))),
    },
  });
}

/** A claimed cloud computer whose host is connected with one authorized device key. */
export async function cloudComputer() {
  const userId = uid();
  const fetch = boat();
  const created = await cc("POST", userId);
  // Find the create among the calls: a relay from an earlier test may still be finishing a wake.
  const [, init] = fetch.mock.calls.find(([url, i]) => i?.method === "POST" && String(url).endsWith("/sandboxes"))!;
  const ticket = JSON.parse(JSON.parse(init!.body as string).files[0].content);
  const host = await newHost();
  expect((await hostClaim(host, ticket)).status).toBe(200);
  const device = ref.signKey();
  const hostWs = await hostSocket(host, [{ dk: ref.b64url(device.pub), grant: null }]);
  return { userId, host, device, hostWs, id: created.body.cloudComputer.cloudComputerId as string };
}

/** Boat as a stopped sandbox: reads answer `archived`, a resume answers `provisioning`. */
export function stoppedBoat() {
  return spyFetch().mockImplementation(async (_url, init) =>
    Response.json({ ok: true, sandbox: { id: "bx_test", state: init?.method === "POST" ? "provisioning" : "archived" } }),
  );
}

export const calls = (spy: ReturnType<typeof stoppedBoat>) => spy.mock.calls.map(([url, init]) => `${init?.method} ${url}`);
export const relay = (host: TestHost) => env.RELAY.get(env.RELAY.idFromName(host.cid));
