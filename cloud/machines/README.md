# codync-cloud

Long-lived, sleepable personal machines on interchangeable providers: the machine layer behind
[Codync Cloud](../../docs/plans/codync-cloud.md), as a standalone package.

Most sandbox SDKs assume a machine is used once and thrown away. This package is for the other case: one
machine per person that lives for months, sleeps when idle, wakes when it's needed and keeps its disk.

**The package decides; the caller acts.** Each provider is a `Driver` (`src/types.ts`): it builds the HTTP requests to
send to that provider and reads the answers back into one vocabulary (`Machine`, `MachineState`, `Outcome` with a
`Failure` in Codync's words). What differs in kind is a member present or not, which the caller reads (`setup`,
`backup`, `deleteHistory`), never the provider's name. It sends nothing, holds no credentials, keeps no state and
starts no timers: the caller adds the base URL and its key, sends the request, and stores what comes back.

## Providers

| Provider | Module | Notes |
|---|---|---|
| [Boat](https://boat.dev) | `boat` | Ubuntu VMs; a stop snapshots the disk (kept 14 days, so a backup before then); a resume restores it, warm or cold; files and setup script at create |
| [InstaBox](https://instabox.sh) | `instabox` | Firecracker machines; sleep is a pause that keeps memory; a stopped or paused disk stays until deleted; files and setup script by `exec` once it runs; keys reach the whole workspace |

## Use

```ts
import { drivers } from "codync-cloud";

const d = drivers.boat;
const req = d.create({ name: "codync-u1", size: "small", files: [], setupScript: "true" }, crypto.randomUUID());
const res = await fetch(`${d.url}${req.path}`, {
  method: req.method,
  headers: { ...req.headers, authorization: `Bearer ${BOAT_API_KEY}`, "content-type": "application/json" },
  body: JSON.stringify(req.body),
});
const machine = d.machine({ status: res.status, body: await res.json() });
```

## Status

0.x, developed inside Codync (`cloud/machines`) under Apache-2.0. The API changes with Codync.
