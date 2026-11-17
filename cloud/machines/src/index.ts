// codync-cloud: long-lived, sleepable personal machines on interchangeable providers.
// The library decides and describes; the caller sends the requests and keeps the state.

import { boat } from "./boat";
import { instabox } from "./instabox";
import type { Driver, ProviderId } from "./types";

export * from "./types";
export { boat, instabox };

/** Every provider's driver, by its id. */
export const drivers: Record<ProviderId, Driver> = { boat, instabox };
