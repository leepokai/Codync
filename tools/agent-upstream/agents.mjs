// How each agent reaches the scripted model: the environment, and any config file, that point its provider at aimock
// on an API key inside the check's own home, and make its shell tool ask before it runs. Add an agent here together
// with its row in accepted.json and its install step in .github/workflows/agent-upstream.yml.

import { execFileSync } from "node:child_process";
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";

/** Environment that points `backend` at the mock on `url`, writing any config it needs under `home`. */
export function pointAtMock(backend, { url, home }) {
  const setup = AGENTS[backend];
  if (!setup) throw new Error(`no aimock setup for ${backend} in agents.mjs`);
  return setup({ url, home });
}

/** `text` at `home`/`relative`, folders made. */
function put(home, relative, text) {
  const file = path.join(home, relative);
  mkdirSync(path.dirname(file), { recursive: true });
  writeFileSync(file, text);
  return file;
}

const AGENTS = {
  claude: ({ url }) => ({ ANTHROPIC_BASE_URL: url, ANTHROPIC_API_KEY: "aimock" }),

  // Codex signed in to ChatGPT ignores OPENAI_BASE_URL: a provider of its own, on an API key, points it at the mock.
  codex: ({ url, home }) => {
    put(home, ".codex/config.toml", [
      'model = "gpt-5.1"', 'model_provider = "aimock"', 'preferred_auth_method = "apikey"', "",
      "[model_providers.aimock]", 'name = "aimock"', `base_url = "${url}/v1"`, 'env_key = "OPENAI_API_KEY"', 'wire_api = "responses"', "",
    ].join("\n"));
    return { CODEX_HOME: path.join(home, ".codex"), OPENAI_API_KEY: "aimock" };
  },

  // Copilot's BYOK provider: no GitHub login, and offline so it reaches nothing but the mock.
  copilot: ({ url }) => ({
    COPILOT_PROVIDER_BASE_URL: `${url}/v1`, COPILOT_PROVIDER_TYPE: "openai", COPILOT_PROVIDER_API_KEY: "aimock",
    COPILOT_MODEL: "gpt-4.1", COPILOT_OFFLINE: "true", COPILOT_AUTO_UPDATE: "false",
  }),

  // Pi reads its providers from ~/.pi/agent/models.json. Pi itself never asks before a command: a gate extension (pi's
  // own permission-gate example) asks through ctx.ui.confirm, which pi-acp turns into the ACP permission request.
  pi: ({ url, home }) => {
    put(home, ".pi/agent/extensions/gate.js", [
      "export default function (pi) {",
      '  pi.on("tool_call", async (event, ctx) => {',
      '    if (event.toolName !== "bash") return undefined;',
      '    const ok = await ctx.ui.confirm("Run this command?", event.input.command);',
      '    return ok ? undefined : { block: true, reason: "Blocked by user" };',
      "  });",
      "}",
      "",
    ].join("\n"));
    const provider = { baseUrl: `${url}/v1`, api: "openai-completions", apiKey: "aimock", models: [{ id: "mock" }] };
    put(home, ".pi/agent/models.json", JSON.stringify({ providers: { aimock: provider } }));
    put(home, ".pi/agent/settings.json", JSON.stringify({ defaultProvider: "aimock", defaultModel: "mock" }));
    return {};
  },

  // oh-my-pi reads the Anthropic key and base URL from the environment, like Claude Code.
  omp: ({ url }) => ({ ANTHROPIC_BASE_URL: url, ANTHROPIC_API_KEY: "aimock" }),

  // OpenCode on a provider of its own, with its free online models off so no other model is offered. No OpenCode mode
  // asks before a shell command by default: the config makes bash ask.
  opencode: ({ url }) => ({
    OPENCODE_CONFIG_CONTENT: JSON.stringify({
      model: "aimock/mock",
      permission: { bash: "ask" },
      provider: { aimock: { npm: "@ai-sdk/openai-compatible", name: "aimock", options: { baseURL: `${url}/v1`, apiKey: "aimock" }, models: { mock: { name: "mock", tool_call: true } } } },
    }),
    OPENCODE_DISABLE_FREE_MODELS: "1",
    OPENCODE_DISABLE_AUTOUPDATE: "1",
  }),

  // Grok Build: a custom model on Chat Completions with its own key, made the default, and xAI's own endpoints moved
  // to the mock, so nothing reaches xAI without a login.
  grok: ({ url, home }) => {
    put(home, ".grok/config.toml", [
      "[cli]", "auto_update = false", "",
      "[models]", 'default = "aimock"', "",
      "[model.aimock]", 'model = "aimock"', 'name = "aimock"', `base_url = "${url}/v1"`, 'api_key = "aimock"', 'api_backend = "chat_completions"', "",
    ].join("\n"));
    return {
      XAI_API_KEY: "aimock", GROK_XAI_API_BASE_URL: `${url}/v1`, GROK_CLI_CHAT_PROXY_BASE_URL: `${url}/v1`,
      GROK_MODELS_BASE_URL: `${url}/v1`, GROK_TELEMETRY_ENABLED: "false",
    };
  },

  // DeepSeek Harness speaks Anthropic Messages to DEEPSEEK_BASE_URL (it appends /v1/messages); telemetry stays off.
  dsh: ({ url }) => ({ DEEPSEEK_BASE_URL: url, DEEPSEEK_API_KEY: "aimock", DSH_TELEMETRY_MODE: "DISABLED" }),

  // Hermes reads its provider from $HERMES_HOME/config.yaml: a custom OpenAI-compatible endpoint on an API key.
  hermes: ({ url, home }) => {
    // Hermes asks only before commands it deems dangerous: a pre_tool_call hook sends its terminal tool to the approval gate.
    const ask = put(home, ".hermes/ask.sh", `#!/bin/sh\necho '{"action": "approve", "message": "Run this command?"}'\n`);
    put(home, ".hermes/config.yaml", [
      "model:", "  provider: custom", "  default: gpt-4o", `  base_url: ${url}/v1`, "  api_key: aimock",
      "hooks:", "  pre_tool_call:", "    - matcher: terminal", `      command: sh ${ask}`, "hooks_auto_accept: true", "",
    ].join("\n"));
    const env = { HOME: home, HERMES_HOME: path.join(home, ".hermes") };
    // A new home builds its dependency environment on first launch, and a second hermes starting meanwhile (the host's
    // sign-in check beside the session) fails with "no dependency environment is committed": build it first.
    execFileSync("hermes", ["acp", "--check"], { env: { ...process.env, ...env }, stdio: "ignore", timeout: 600_000 });
    return env;
  },

  // Antigravity's ACP server takes its auth method from settings.json under the Gemini home, and its Gemini client
  // (google-genai) the endpoint from GOOGLE_GEMINI_BASE_URL.
  antigravity: ({ url, home }) => {
    put(home, ".gemini/antigravity-acp/settings.json", JSON.stringify({ auth: { type: "gemini-api-key" } }));
    return { GEMINI_API_KEY: "aimock", GOOGLE_GEMINI_BASE_URL: url };
  },
};
