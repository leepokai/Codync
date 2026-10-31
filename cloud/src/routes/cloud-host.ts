// Codync Cloud, the host's side (docs/plans/codync-cloud.md): an idle cloud computer asks to sleep, and is
// offered a newer release to install instead (the cloud soft update); a host still running after its request
// asks whether it was woken. Urgent updates reach it in `/v1/host/state` (`urgentUpdate`).

import { boat } from "../../machines/src";
import { softUpdate } from "../app-versions";
import { machineFor, providerSend, setState } from "../cloud-machines";
import { value } from "./cloud-computers";
import { ApiError, body, host, type Ctx } from "./common";

/**
 * `POST /v1/host/sleep {wakeAt}` (Sig(host)): an idle cloud computer asks to be stopped. Answers
 * `{update}` without stopping it when a newer release suits every device of the account (installed now, so a
 * wake never lands on a restart); refused with `conflict` while a device is connected to its relay; otherwise
 * the relay wakes it at `wakeAt` or when needed.
 */
export async function hostSleep(c: Ctx) {
  const comp = await host(c);
  const wakeAt = body(c).wakeAt ?? null;
  if (wakeAt !== null && (typeof wakeAt !== "number" || !Number.isSafeInteger(wakeAt))) throw new ApiError("badRequest", "Invalid wakeAt");
  const row = c.env.BOAT_API_KEY ? await machineFor(c.env, comp.id) : null;
  if (!row) throw new ApiError("notFound");
  const update = await softUpdate(c.env, comp, c.now);
  if (update) return { update };
  const res = await c.env.RELAY.get(c.env.RELAY.idFromName(comp.id)).fetch("https://do/internal/sleep", {
    method: "POST",
    headers: { "X-Codync-Internal": "1", "Content-Type": "application/json" },
    body: JSON.stringify({ wakeAt }),
  });
  if (res.status === 409) throw new ApiError("conflict", "A device is connected.");
  if (!res.ok) throw new ApiError("internal");
  value(boat.machine(await providerSend(c.env, boat.stop(row.provider_id))));
  await setState(c.env, row.id, "stopping", c.now);
  return {};
}

/** `GET /v1/host/sleep` (Sig(host)): the machine's state as the cloud last saw it. A host still running after
 * asking to sleep polls it, and reconnects to its relay once a wake has started the machine again. */
export async function hostSleepState(c: Ctx) {
  const comp = await host(c);
  const row = await machineFor(c.env, comp.id);
  if (!row) throw new ApiError("notFound");
  return { state: row.state };
}
