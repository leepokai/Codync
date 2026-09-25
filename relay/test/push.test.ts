// Alert aps: `mutableContent` passes through as `mutable-content: 1`, and only when asked.
import assert from "node:assert/strict";
import { alertAps } from "../src/index.ts";

const base = { alert: { title: "Codync", body: "Needs you" }, threadId: "b1", category: "needsInput" };
assert.equal(alertAps({ ...base, mutableContent: true })["mutable-content"], 1);
assert.equal("mutable-content" in alertAps(base), false);
assert.equal("mutable-content" in alertAps({ ...base, mutableContent: false }), false);
assert.deepEqual(alertAps(base).alert, { title: "Codync", body: "Needs you" });
console.log("push tests ok");
