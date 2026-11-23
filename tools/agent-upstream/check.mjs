// The upstream check: each adapter in accepted.json whose ACP registry version moved (every one with --all) runs one
// turn through a real Codync host, with a scripted model (aimock) standing in for the provider: no login, no tokens.
// The host downloads the registry's latest build itself, as every computer running Codync does within a day.
//
//   node check.mjs <codync-host binary> [--all]
//
// Exits 1 when an adapter fails; with GITHUB_STEP_SUMMARY set, writes a table of what ran.

import { LLMock } from "@copilotkit/aimock";
import { execFile, spawn } from "node:child_process";
import { appendFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { promisify } from "node:util";

const run = promisify(execFile);
const REGISTRY = "https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json";
const TURN_TIMEOUT = 8 * 60_000;

const [hostBin, ...flags] = process.argv.slice(2);
if (!hostBin) throw new Error("usage: node check.mjs <codync-host> [--all]");
const all = flags.includes("--all");
const accepted = JSON.parse(readFileSync(new URL("accepted.json", import.meta.url), "utf8")).agents;
const registry = await (await fetch(REGISTRY, { signal: AbortSignal.timeout(30_000) })).json();
const latest = Object.fromEntries(registry.agents.map((agent) => [agent.id, agent.version]));

const rows = [];
for (const [id, { backend, version }] of Object.entries(accepted)) {
  const now = latest[id];
  if (!now) {
    rows.push({ id, version, now: "missing", result: "failed", detail: "no longer in the ACP registry" });
    continue;
  }
  if (!all && now === version) {
    rows.push({ id, version, now, result: "unchanged" });
    continue;
  }
  console.log(`${id} ${version} -> ${now}: one turn on ${backend}`);
  try {
    const reply = await oneTurn(backend);
    rows.push({ id, version, now, result: "passed", detail: reply });
  } catch (error) {
    rows.push({ id, version, now, result: "failed", detail: error.message });
  }
}

const table = [
  "| Adapter | Accepted | Registry | Result | Detail |",
  "| --- | --- | --- | --- | --- |",
  ...rows.map((r) => `| ${r.id} | ${r.version} | ${r.now} | ${r.result} | ${(r.detail ?? "").replace(/[|\n]/g, " ").slice(0, 300)} |`),
].join("\n");
console.log(table);
if (process.env.GITHUB_STEP_SUMMARY) {
  const moved = rows.filter((r) => r.result === "passed" && r.now !== r.version).map((r) => `${r.id} to ${r.now}`);
  const accept = moved.length ? `\n\nPassed on a new version: raise ${moved.join(", ")} in tools/agent-upstream/accepted.json.` : "";
  appendFileSync(process.env.GITHUB_STEP_SUMMARY, `## Upstream agents\n\n${table}${accept}\n`);
}
if (rows.some((r) => r.result === "failed")) process.exitCode = 1;

/** One turn of a new bot on `backend`, in a host of its own with a home of its own, against a model that answers "ok". */
async function oneTurn(backend) {
  const mock = new LLMock({ port: 0 });
  // Every request, whatever its shape (titles, tool results), gets the same answer: an unanswered one is retried.
  mock.on({ predicate: () => true }, { content: "ok" });
  await mock.start();
  const root = mkdtempSync(path.join(tmpdir(), "cu-"));
  const [home, data, codexHome, folder] = ["h", "d", "x", "w"].map((dir) => path.join(root, dir));
  for (const dir of [home, data, codexHome, folder]) mkdirSync(dir, { recursive: true });
  // Codex signed in to ChatGPT ignores OPENAI_BASE_URL: a provider of its own, on an API key, points it at the mock.
  writeFileSync(path.join(codexHome, "config.toml"), [
    'model = "gpt-5.1"', 'model_provider = "aimock"', 'preferred_auth_method = "apikey"', "",
    "[model_providers.aimock]", 'name = "aimock"', `base_url = "${mock.url}/v1"`, 'env_key = "OPENAI_API_KEY"', 'wire_api = "responses"', "",
  ].join("\n"));
  const port = String(19500 + Math.floor(Math.random() * 400));
  const env = {
    ...process.env, HOME: home, CODYNC_HOME: data, CODYNC_CLOUD: "off", CODYNC_ANALYTICS_ENV: "dev",
    ANTHROPIC_BASE_URL: mock.url, ANTHROPIC_API_KEY: "aimock", CODEX_HOME: codexHome, OPENAI_API_KEY: "aimock",
  };
  const host = spawn(hostBin, ["serve", "--bind", "127.0.0.1", "--port", port], { env, stdio: ["ignore", "pipe", "pipe"] });
  let log = "";
  host.stdout.on("data", (d) => (log += d));
  host.stderr.on("data", (d) => (log += d));
  const api = async (method, body = {}) => {
    const token = readFileSync(path.join(data, "token"), "utf8").trim();
    const response = await fetch(`http://127.0.0.1:${port}/api/${method}`, {
      method: "POST",
      headers: { authorization: `Bearer ${token}`, "content-type": "application/json" },
      body: JSON.stringify(body),
    });
    const text = await response.text();
    if (!response.ok) throw new Error(`${method}: ${response.status} ${text}`);
    return JSON.parse(text || "null");
  };
  try {
    // The registry loads after the host starts: until then its agents aren't available.
    for (let i = 0; ; i++) {
      const hello = await api("hello").catch(() => null);
      if (hello?.backends?.find((b) => b.id === backend)?.available) break;
      if (i > 90) throw new Error(`${backend} never became available`);
      await new Promise((resolve) => setTimeout(resolve, 2000));
    }
    const bot = await api("createBot", { name: "Upstream check", backend, folder });
    const sent = await run(hostBin, ["send", "--port", port, "--bot", bot.id ?? bot.bot?.id, "--wait", "--json", "Reply with exactly the word: ok"], { env, timeout: TURN_TIMEOUT });
    const answer = JSON.parse(sent.stdout);
    if (answer.status !== "completed" || !/\bok\s*$/u.test(answer.reply?.split("\n\n>")[0] ?? "")) {
      throw new Error(`the turn ended ${answer.status} (${answer.stopReason}): ${answer.reply}`);
    }
    return `${answer.stopReason} in ${Math.round(answer.elapsedSeconds)} s`;
  } catch (error) {
    throw new Error(`${error.message}\n${log.slice(-1500)}`);
  } finally {
    host.kill("SIGTERM");
    await mock.stop();
    rmSync(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 500 });
  }
}
