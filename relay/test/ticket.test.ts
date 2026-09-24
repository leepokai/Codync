// Ticket round-trip: sealed tickets open with the same key and fail with another.
import assert from "node:assert/strict";
import { sealTicket, openTicket } from "../src/index.ts";

const key = btoa(String.fromCharCode(...crypto.getRandomValues(new Uint8Array(32))));
const env = { TICKET_KEY: key } as any;
const ticket = await sealTicket(env, { t: "ab".repeat(32), e: "sandbox", k: "alert" });
assert.deepEqual(await openTicket(env, ticket), { t: "ab".repeat(32), e: "sandbox", k: "alert" });
const other = { TICKET_KEY: btoa(String.fromCharCode(...new Uint8Array(32))) } as any;
assert.equal(await openTicket(other, ticket), null);
assert.equal(await openTicket(env, ticket.slice(0, -2) + "AA"), null);
console.log("ticket tests ok");
