import assert from "node:assert/strict";
import { FcmClient, FcmError, fcmClient } from "../src/fcm.ts";
import worker, { fcmData, latestTicketIndices, openTicket } from "../src/index.ts";

const pair = await crypto.subtle.generateKey({ name: "RSASSA-PKCS1-v1_5", modulusLength: 2048,
  publicExponent: new Uint8Array([1, 0, 1]), hash: "SHA-256" }, true, ["sign", "verify"]);
const keyBytes = await crypto.subtle.exportKey("pkcs8", pair.privateKey);
const account = { project_id: "test-codync", client_email: "push@test-codync.iam.gserviceaccount.com",
  private_key: "-----BEGIN PRIVATE KEY-----\n" + btoa(String.fromCharCode(...new Uint8Array(keyBytes))) + "\n-----END PRIVATE KEY-----" };
for (const invalid of ["private-secret-not-json", "null", "{}", '{"project_id":42}']) {
  assert.throws(() => fcmClient(invalid), error => error instanceof FcmError && error.reason === "invalid FCM service account");
}
let exchanges = 0;
let sends = 0;
let failure: { status: number; code: string; detail?: string } | undefined;
const sender: typeof fetch = async function(this: unknown, url, options) {
  assert.equal(this, globalThis);
  if (String(url) === "https://oauth2.googleapis.com/token") {
    exchanges++;
    const fields = new URLSearchParams(String(options?.body));
    const assertion = fields.get("assertion")!;
    const parts = assertion.split(".");
    const decode = (value: string) => Uint8Array.from(Buffer.from(value, "base64url"));
    const verified = await crypto.subtle.verify("RSASSA-PKCS1-v1_5", pair.publicKey, decode(parts[2]), new TextEncoder().encode(parts[0] + "." + parts[1]));
    assert.equal(verified, true);
    const claims = JSON.parse(Buffer.from(parts[1], "base64url").toString());
    assert.equal(claims.scope, "https://www.googleapis.com/auth/firebase.messaging");
    assert.equal(claims.aud, "https://oauth2.googleapis.com/token");
    assert.equal(claims.exp - claims.iat, 3600);
    return Response.json({ access_token: "test-access", expires_in: 3600 });
  }
  assert.equal(String(url), "https://fcm.googleapis.com/v1/projects/test-codync/messages:send");
  assert.equal(new Headers(options?.headers).get("Authorization"), "Bearer test-access");
  const payload = JSON.parse(String(options?.body));
  assert.equal(payload.message.fid, "CaseSensitiveToken");
  assert.equal("notification" in payload.message, false);
  assert.deepEqual(payload.message.data, { sealed: "ciphertext" });
  sends++;
  if (failure) return Response.json({ error: { status: failure.code, details: failure.detail ? [
    { "@type": "type.googleapis.com/google.firebase.fcm.v1.FcmError", errorCode: failure.detail },
  ] : [] } }, { status: failure.status });
  return Response.json({ name: "projects/test-codync/messages/test" });
};
const client = new FcmClient(account, sender);
await Promise.all([client.push("CaseSensitiveToken", { sealed: "ciphertext" }, true), client.push("CaseSensitiveToken", { sealed: "ciphertext" }, false)]);
assert.equal(exchanges, 1);
assert.equal(sends, 2);
failure = { status: 404, code: "NOT_FOUND", detail: "UNREGISTERED" };
await assert.rejects(client.push("CaseSensitiveToken", { sealed: "ciphertext" }, true), error => error instanceof FcmError && error.gone);
for (const error of [
  { status: 400, code: "INVALID_ARGUMENT", detail: "INVALID_ARGUMENT" },
  { status: 403, code: "PERMISSION_DENIED", detail: "SENDER_ID_MISMATCH" },
  { status: 404, code: "NOT_FOUND" },
  { status: 503, code: "UNAVAILABLE" },
]) {
  failure = error;
  await assert.rejects(client.push("CaseSensitiveToken", { sealed: "ciphertext" }, true), error => error instanceof FcmError && !error.gone);
}
await assert.rejects(client.push("CaseSensitiveToken", { sealed: "x".repeat(4096) }, true), /payload too large/);
assert.equal(exchanges, 1);

const ticket = { p: "fcm" as const, t: "CaseSensitiveToken", k: "alert" as const, c: "local", h: "A".repeat(22) };
const alert = { alert: { title: "Codync", body: "A bot needs you" }, data: { botId: "bot", computerId: ticket.h, ctx: "local", sealed: "ciphertext" } };
assert.equal(fcmData(ticket, alert).sealed, "ciphertext");
assert.throws(() => fcmData(ticket, { ...alert, data: { ...alert.data, ctx: "other" } }));
assert.throws(() => fcmData(ticket, { ...alert, data: { ...alert.data, sealed: undefined } }));
const task = { ...ticket, k: "liveactivity" as const, b: "bot", r: "00112233-4455-6677-8899-aabbccddeeff" };
const taskData = fcmData(task, { liveActivity: { event: "update", contentState: { status: "working", activity: "" } } });
assert.equal(taskData.kind, "task");
assert.equal(taskData.botId, "bot");
assert.equal("activity" in taskData, false);
assert.throws(() => fcmData(task, { liveActivity: { event: "update", contentState: { status: "working", activity: "private" } } }));
assert.deepEqual([...latestTicketIndices([
  ticket, { ...ticket, t: "casesensitivetoken" }, { ...ticket },
  task, { ...task, b: "other-bot" },
  { t: "ab", e: "sandbox", k: "alert" }, { t: "AB", e: "sandbox", k: "alert" },
])], [2, 1, 3, 4, 6]);

const env = { TICKET_KEY: btoa(String.fromCharCode(...crypto.getRandomValues(new Uint8Array(32)))) } as any;
const registered = await worker.fetch(new Request("https://relay.test/register", { method: "POST", body: JSON.stringify({
  provider: "fcm", token: "A_CaseSensitiveFCMToken:123", kind: "alert", ctx: "local", computerId: ticket.h,
}) }), env);
assert.equal(registered.status, 200);
const result = await registered.json() as { ticket: string };
const opened = await openTicket(env, result.ticket);
assert.equal(opened?.p, "fcm");
assert.equal(opened?.t, "A_CaseSensitiveFCMToken:123");
console.log("FCM ticket, payload, OAuth and error tests ok");
