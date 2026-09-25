import { generateKeyPairSync, randomBytes } from "node:crypto";
import { cloudflareTest, readD1Migrations } from "@cloudflare/vitest-pool-workers";
import { defineConfig } from "vitest/config";

// A throwaway Clerk signing key: the Worker verifies networklessly with the public half (CLERK_JWT_KEY),
// the tests mint session tokens with the private half.
const { publicKey, privateKey } = generateKeyPairSync("rsa", { modulusLength: 2048 });

export default defineConfig(async () => ({
  plugins: [
    cloudflareTest({
      wrangler: { configPath: "./wrangler.toml", environment: "dev" },
      miniflare: {
        // The pool's bundled workerd trails wrangler's; pin to the newest date it supports.
        compatibilityDate: "2026-08-22",
        bindings: {
          TEST_MIGRATIONS: await readD1Migrations("./migrations"),
          CLERK_JWT_KEY: publicKey.export({ type: "spki", format: "pem" }).toString(),
          TEST_CLERK_PRIVATE_JWK: JSON.stringify(privateKey.export({ format: "jwk" })),
          CLERK_WEBHOOK_SECRET: `whsec_${randomBytes(24).toString("base64")}`,
        },
      },
    }),
  ],
  test: {
    setupFiles: ["./test/setup.ts"],
    testTimeout: 20_000,
  },
}));
