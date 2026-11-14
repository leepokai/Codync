// Codync Cloud test helpers: Boat stubbed with a fetch spy, the Worker's cloud computer routes, the user's own Boat
// key, and a claimed cloud computer with its host connected.

import { runDurableObjectAlarm } from "cloudflare:test";
import { expect, vi } from "vitest";
import { saveCredential } from "../src/cloud-credentials";
import worker, { type Env } from "../src/index";
import { AUTHORITY, ORIGIN, call, clerkToken, env, hostSocket, newHost, type Json, type TestHost } from "./helpers";
import * as ref from "./ref";

export const uid = () => `user_cc_${ref.b64url(ref.random(6))}`;

/** The Boat key every test user brings. */
export const KEY = "boat_live_test9abc";

/** A signed or Clerk call through the Worker with some env overridden. */
export async function callWith(over: Partial<Env>, method: string, path: string, o: { token?: string; key?: ref.SignKey; body?: unknown; app?: string } = {}) {
  const bytes = o.body === undefined ? new Uint8Array() : ref.enc.encode(JSON.stringify(o.body));
  const headers: Record<string, string> = {};
  if (o.token) headers.Authorization = `Bearer ${o.token}`;
  if (o.app) headers["Codync-App"] = o.app;
  if (o.key) headers["Codync-Sig"] = ref.signRequest(o.key, { method, authority: AUTHORITY, pathAndQuery: path, body: bytes });
  const res = await worker.fetch(new Request(ORIGIN + path, { method, headers, body: bytes.length ? bytes : undefined }), { ...env, ...over }, {
    waitUntil: () => {},
  } as unknown as ExecutionContext);
  return { status: res.status, body: (await res.json()) as Json };
}

/** Calls `/v1/cloud-computers` (or `path`) as `userId`; `body` goes as JSON. */
export const cc = async (method: string, userId: string, path = "/v1/cloud-computers", body?: unknown) =>
  callWith({}, method, path, { token: await clerkToken(userId), body });

/** Gives `userId` a Boat key, as saving one does once Boat took it (the account row comes with the first call). */
export async function withKey(userId: string) {
  await cc("GET", userId);
  await saveCredential(env, userId, "boat", KEY, Date.now());
}

/** A fresh fetch spy (spying again returns the same spy, so earlier calls are cleared). */
export const spyFetch = () => vi.spyOn(globalThis, "fetch").mockClear();

type Answer = [status: number, body: unknown];

/** Boat refusing a call with `code`. */
export const forbidden = (code: string): Answer => [403, { ok: false, code, message: code }];

/** Stubs Boat: `answers` by path and query (an answer, or one for the key sent, `undefined` for the usual), every
 * other call a sandbox in `state`; each request is recorded. A key's webhooks are out of reach, as for the key a user
 * makes for Codync. */
export function boat(state = "provisioning", answers: Record<string, Answer | ((key: string) => Answer | undefined)> = {}) {
  const answer: typeof answers = { "/webhooks": forbidden("api_key_action_forbidden"), ...answers };
  return spyFetch().mockImplementation(async (url, init) => {
    const { pathname, search } = new URL(String(url));
    const a = answer[pathname.replace("/api/v1", "") + search];
    const key = (init?.headers as Record<string, string> | undefined)?.authorization?.replace("Bearer ", "") ?? "";
    const [status, body] = (typeof a === "function" ? a(key) : a) ?? [200, { ok: true, sandbox: { id: "bx_test", state } }];
    return Response.json(body, { status });
  });
}

/** The host's side of the ticket: the claim signed with its key, sent to `/v1/host/claim`. */
/** `extra`: more of the claim body (a cloud computer's `keeperKey`). */
export function hostClaim(host: TestHost, ticket: { claimId: string; nonce: string; userId: string }, extra: Record<string, unknown> = {}) {
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
      ...extra,
    },
  });
}

/** A claimed cloud computer whose host is connected with one authorized device key. */
export async function cloudComputer() {
  const userId = uid();
  await withKey(userId);
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

/** Runs `host`'s relay alarm as if `ms` had passed (only `Date` moves). */
export async function alarmAfter(host: TestHost, ms: number) {
  vi.useFakeTimers({ toFake: ["Date"], now: Date.now() + ms });
  try {
    await runDurableObjectAlarm(relay(host));
  } finally {
    vi.useRealTimers();
  }
}

/** The InstaBox key test users bring (`ibk_` and 52 of a–z, 2–7). */
export const IBX_KEY = `ibk_${"a2".repeat(26)}`;

/** Gives `userId` an InstaBox key, as saving one does once InstaBox took it. */
export async function withInstaboxKey(userId: string) {
  await cc("GET", userId);
  await saveCredential(env, userId, "instabox", IBX_KEY, Date.now());
}

/** Stubs InstaBox: `answers` by "METHOD /path?query" first; then a personal key (not a workspace admin's), a machine
 * in `state` (ready or not) for reads, an exec that exits 0, and an accepted operation for every change. */
export function instaboxApi(state = "running", answers: Record<string, Answer> = {}, ready = true) {
  return spyFetch().mockImplementation(async (url, init) => {
    const { pathname, search } = new URL(String(url));
    const method = init?.method ?? "GET";
    const a = answers[`${method} ${pathname}${search}`];
    if (a) return Response.json(a[1], { status: a[0] });
    if (pathname === "/v1/me") return Response.json({ orgId: "org_1" });
    if (pathname.endsWith("/invites")) return Response.json({ error: { code: "missing_scope" } }, { status: 403 });
    if (pathname.endsWith("/exec")) return Response.json({ stdout: "", stderr: "", exitCode: 0 });
    if (method === "GET") return Response.json({ id: "m_test", state, runtime: { ready } });
    const type = method === "DELETE" ? "delete" : pathname === "/v1/machines" ? "create" : pathname.split("/").pop();
    return Response.json({ id: "op_1", machineId: "m_test", type, state: "queued", error: null }, { status: 202 });
  });
}

/** The claim ticket an InstaBox setup exec writes to `.codync/managed.json`. */
export function instaboxTicket(spy: ReturnType<typeof instaboxApi>) {
  const [, init] = spy.mock.calls.find(([url]) => String(url).endsWith("/exec"))!;
  const stdin: string = JSON.parse(init!.body as string).stdin;
  const b64 = /printf %s '([A-Za-z0-9+/=]+)' \| base64 -d > '\.codync\/managed\.json'/.exec(stdin)![1]!;
  return JSON.parse(new TextDecoder().decode(Uint8Array.from(atob(b64), (c) => c.charCodeAt(0))));
}

/** Boat (`boat` in `boatState`) and InstaBox stubbed at once, each answering its own calls; `instabox` answers InstaBox's
 * (`instaboxApi`'s by default, or a provider out of reach). */
export function bothProviders(boatState = "ready", instabox = instaboxApi().getMockImplementation()!) {
  const answer = boat(boatState).getMockImplementation()!;
  return spyFetch().mockImplementation((url, init) => (String(url).startsWith("https://boat.dev") ? answer(url, init) : instabox(url, init)));
}

/** A user with a cloud computer on each provider: Boat's first, then InstaBox's, named "Build box". */
export async function onBothProviders() {
  const userId = uid();
  await withKey(userId);
  await withInstaboxKey(userId);
  boat();
  await cc("POST", userId, "/v1/cloud-computers", { provider: "boat" });
  instaboxApi();
  await cc("POST", userId, "/v1/cloud-computers", { provider: "instabox", name: "Build box" });
  return userId;
}
