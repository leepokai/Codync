import { describe, expect, it } from "vitest";
import { call, clerkToken } from "./helpers";

describe("device-local computer order", () => {
  it("does not expose account-wide ordering endpoints", async () => {
    const id = `user_${crypto.randomUUID()}`;
    const token = await clerkToken(id);
    const path = `/v1/accounts/${id}/computer-order`;
    expect((await call("GET", path, { token })).status).toBe(404);
    expect((await call("PUT", path, { token, body: { computerIds: ["mini", "ssh"] } })).status).toBe(404);
  });
});
