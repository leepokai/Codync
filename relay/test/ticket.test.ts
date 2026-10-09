// Ticket round-trip: sealed tickets open with the same key and fail with another.
import assert from "node:assert/strict";
import worker, { sealTicket, openTicket, apnsTopic, latestTicketIndices } from "../src/index.ts";

const key = btoa(String.fromCharCode(...crypto.getRandomValues(new Uint8Array(32))));
const env = { TICKET_KEY: key } as any;
const ticket = await sealTicket(env, { t: "ab".repeat(32), e: "sandbox", k: "alert" });
assert.deepEqual(await openTicket(env, ticket), { t: "ab".repeat(32), e: "sandbox", k: "alert" });
const other = { TICKET_KEY: btoa(String.fromCharCode(...new Uint8Array(32))) } as any;
assert.equal(await openTicket(other, ticket), null);
assert.equal(await openTicket(env, ticket.slice(0, -2) + "AA"), null);
console.log("ticket tests ok");

const registration = { token: "ab".repeat(32), env: "sandbox", kind: "alert", bundleId: "com.pokai.Codync.ios.dev" };
const response = await worker.fetch(new Request("https://relay.test/register", { method: "POST", body: JSON.stringify(registration) }), env);
assert.equal(response.status, 200);
const registered = await response.json() as { ticket: string };
assert.equal((await openTicket(env, registered.ticket))?.b, registration.bundleId);
assert.equal(apnsTopic(undefined, "alert"), "com.pokai.Codync.ios");
assert.equal(apnsTopic(registration.bundleId, "liveactivity"), "com.pokai.Codync.ios.dev.push-type.liveactivity");
assert.throws(() => apnsTopic("another.app", "alert"));
const invalid = await worker.fetch(new Request("https://relay.test/register", { method: "POST", body: JSON.stringify({ ...registration, bundleId: "another.app" }) }), env);
assert.equal(invalid.status, 400);
assert.deepEqual([...latestTicketIndices([
  { t: "ab", e: "sandbox", k: "alert" },
  { t: "ab", e: "sandbox", k: "alert", b: registration.bundleId },
])], [0, 1]);
console.log("development push destinations stay isolated; existing tickets retain production topic");
