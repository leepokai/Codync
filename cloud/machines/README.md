# codync-cloud

Long-lived, sleepable personal machines on interchangeable providers: the machine layer behind
[Codync Cloud](../../docs/plans/codync-cloud.md), as a standalone package.

Most sandbox SDKs assume a machine is used once and thrown away. This package is for the other case: one
machine per person that lives for months, sleeps when idle, wakes when it's needed and keeps its disk.

**The package decides; the caller acts.** It builds the HTTP requests to send to a provider and reads the
answers back into one vocabulary (`Machine`, `MachineState`, `Outcome`). It sends nothing, holds no
credentials, keeps no state and starts no timers: the caller adds the base URL and its key, sends the
request, and stores what comes back.

## Providers

| Provider | Module | Notes |
|---|---|---|
| [Boat](https://boat.dev) | `boat` | Ubuntu VMs; a stop snapshots the disk (kept 14 days); a resume restores it, warm or cold |

## Use

```ts
import { boat } from "codync-cloud";

const req = boat.create({ name: "codync-u1", size: "small", files: [], setupScript: "true" }, crypto.randomUUID());
const res = await fetch(`https://boat.dev/api/v1${req.path}`, {
  method: req.method,
  headers: { ...req.headers, authorization: `Bearer ${BOAT_API_KEY}`, "content-type": "application/json" },
  body: JSON.stringify(req.body),
});
const machine = boat.machine({ status: res.status, body: await res.json() });
```

## Status

0.x, developed inside Codync (`cloud/machines`) under Apache-2.0. The API changes with Codync.
