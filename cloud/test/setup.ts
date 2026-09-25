import { applyD1Migrations } from "cloudflare:test";
import { env } from "cloudflare:workers";
import type { TestEnv } from "./helpers";

await applyD1Migrations((env as unknown as TestEnv).DB, (env as unknown as TestEnv).TEST_MIGRATIONS);
