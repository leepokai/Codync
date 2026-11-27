// The upstream check: each adapter in accepted.json whose ACP registry version moved (every one with --all) runs one
// scripted turn through a real Codync host, with aimock standing in for the provider: no login, no tokens. The host
// downloads the registry's latest build itself, as every computer running Codync does within a day.
//
// The turn (scenario.mjs) has the agent run a shell command that writes a file, in a bot on the agent's asking mode,
// and passes when the transcript shows the tool call, its permission card was answered, the file was written and the
// reply came back, and no agent process outlives the stopped host.
//
//   node check.mjs <codync-host binary> [--all] [--only <backend>]
//
// Exits 1 when an adapter fails; with GITHUB_STEP_SUMMARY set, writes a table of what ran.

import { LLMock } from "@copilotkit/aimock";
import { execFile, spawn } from "node:child_process";
import { appendFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { promisify } from "node:util";
import { pointAtMock } from "./agents.mjs";
import { stopDaemons, stopHost } from "./processes.mjs";
import { PROMPT, respond } from "./scenario.mjs";

const run = promisify(execFile);
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
const REGISTRY = "https://cdn.agentclientprotocol.com/registry/v1/latest/registry.json";
const TURN_TIMEOUT_SECONDS = 300;

const [hostBin, ...flags] = process.argv.slice(2);
if (!hostBin) throw new Error("usage: node check.mjs <codync-host> [--all] [--only <backend>]");
const all = flags.includes("--all");
const only = flags.includes("--only") ? flags[flags.indexOf("--only") + 1] : null;
const accepted = JSON.parse(readFileSync(new URL("accepted.json", import.meta.url), "utf8")).agents;
const registry = await (await fetch(REGISTRY, { signal: AbortSignal.timeout(30_000) })).json();
const latest = Object.fromEntries(registry.agents.map((agent) => [agent.id, agent.version]));

const rows = [];
for (const [id, { backend, version = "-", mode, registry: listed = true }] of Object.entries(accepted)) {
  if (only && only !== backend) continue;
  // An agent outside the ACP registry runs on its installed CLI: no version to follow, so it runs every time.
  const now = listed ? latest[id] : "installed";
  if (!now) {
    rows.push({ id, version, now: "missing", result: "failed", detail: "no longer in the ACP registry" });
    continue;
  }
  if (listed && !all && !only && now === version) {
    rows.push({ id, version, now, result: "unchanged" });
    continue;
  }
  console.log(`${id} ${version} -> ${now}: the scripted turn on ${backend}`);
  try {
    rows.push({ id, version, now, result: "passed", detail: await scriptedTurn(backend, mode) });
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
// The table cuts each detail short: a failure's whole message (with the host's log) follows it.
for (const r of rows.filter((row) => row.result === "failed")) console.log(`\n## ${r.id} failed\n${r.detail}`);
if (process.env.GITHUB_STEP_SUMMARY) {
  const moved = rows.filter((r) => r.result === "passed" && r.now !== r.version && r.now !== "installed").map((r) => `${r.id} to ${r.now}`);
  const accept = moved.length ? `\n\nPassed on a new version: raise ${moved.join(", ")} in tools/agent-upstream/accepted.json.` : "";
  appendFileSync(process.env.GITHUB_STEP_SUMMARY, `## Upstream agents\n\n${table}${accept}\n`);
}
if (rows.some((r) => r.result === "failed")) process.exitCode = 1;

/** The scripted turn of a new bot on `backend` in `mode`, in a host of its own with a home of its own. */
async function scriptedTurn(backend, mode) {
  const mock = new LLMock({ port: 0, responsesTools: "extended" });
  mock.on({ predicate: () => true }, respond);
  const root = mkdtempSync(path.join(tmpdir(), "cu-"));
  await mock.start();
  const [home, data] = ["h", "d"].map((dir) => path.join(root, dir));
  const port = String(19500 + Math.floor(Math.random() * 400));
  let host;
  let log = "";
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
    for (const dir of [home, data]) mkdirSync(dir, { recursive: true });
    const base = { ...withoutCredentials(process.env), HOME: home, CODYNC_HOME: data, CODYNC_CLOUD: "off", CODYNC_ANALYTICS_ENV: "dev" };
    const env = { ...base, ...pointAtMock(backend, { url: mock.url, home, env: base }) };
    host = spawn(hostBin, ["serve", "--bind", "127.0.0.1", "--port", port], { env, stdio: ["ignore", "pipe", "pipe"] });
    host.stdout.on("data", (d) => (log += d));
    host.stderr.on("data", (d) => (log += d));
    // The registry loads after the host starts: until then its agents aren't available.
    for (let i = 0; ; i++) {
      const hello = await api("hello").catch(() => null);
      if (hello?.backends?.find((b) => b.id === backend)?.available) break;
      if (i > 90) throw new Error(`${backend} never became available`);
      await sleep(2000);
    }
    // A bot works in its own folder, which the host makes: the command writes there.
    const { bot } = await api("createBot", { name: "Upstream check", backend, ...(mode ? { mode } : {}) });
    const folder = bot.cwd;
    writeFileSync(path.join(folder, "notes.txt"), "alpha\n");
    const approving = approveCards(api, bot.id);
    const sent = await run(hostBin, ["send", "--port", port, "--bot", bot.id, "--wait", "--timeout", String(TURN_TIMEOUT_SECONDS), "--json", PROMPT], { env })
      .catch((error) => error);
    approving.stop();
    const answer = JSON.parse(sent.stdout || "{}");
    const { entries } = await api("history", { botId: bot.id });
    const failed = verdict({ answer, entries, file: readFileSync(path.join(folder, "notes.txt"), "utf8"), cards: await approving.done });
    if (failed) throw new Error(failed);
    const leftover = await stopHost(host);
    if (leftover) throw new Error(`agent processes outlived the host: ${leftover}`);
    return `${answer.stopReason} in ${Math.round(answer.elapsedSeconds)} s`;
  } catch (error) {
    throw new Error(`${error.message}\n${log.slice(-1500)}`);
  } finally {
    host?.kill("SIGTERM");
    await stopDaemons(home);
    await mock.stop();
    rmSync(root, { recursive: true, force: true, maxRetries: 5, retryDelay: 500 });
  }
}

/** What's missing from a scripted turn, or null when it all happened. */
function verdict({ answer, entries, file, cards }) {
  const tool = entries.find((e) => e.kind === "tool");
  const reply = entries.findLast((e) => e.kind === "agent" && e.data.final);
  if (answer.status !== "completed") {
    const notices = entries.filter((e) => e.kind === "notice").map((e) => e.data.text ?? JSON.stringify(e.data));
    return `the turn ended ${answer.status} (${answer.stopReason}): ${answer.reply || answer.error || ""} ${notices.join(" / ")}`;
  }
  if (!tool) return `no tool call in the transcript (${entries.map((e) => e.kind).join(", ")})`;
  if (tool.data.status !== "completed") return `the tool call ended ${tool.data.status}`;
  if (!cards) return "the agent never asked to run the command: no permission card";
  if (file.trim() !== "beta") return `notes.txt reads ${JSON.stringify(file)} after the command`;
  if (!/done:/u.test(reply?.data.text ?? "")) return `the reply missed the tool result: ${reply?.data.text}`;
  return null;
}

/** Answers each permission card of `botId` with its allow-once option, as a user would; `done` is how many it answered. */
function approveCards(api, botId) {
  let running = true;
  const answered = new Set();
  const done = (async () => {
    while (running) {
      const { entries } = await api("history", { botId }).catch(() => ({ entries: [] }));
      for (const card of entries.filter((e) => e.kind === "permission" && e.data.status === "pending" && !answered.has(e.id))) {
        const allow = card.data.options.find((o) => o.kind === "allow_once");
        if (!allow) throw new Error(`the permission card has no allow-once option: ${JSON.stringify(card.data.options)}`);
        await api("respondPermission", { entryId: card.id, optionId: allow.optionId });
        answered.add(card.id);
      }
      await sleep(500);
    }
    return answered.size;
  })();
  // Awaited only once the turn is over: a failed answer until then fails the turn, not the whole check.
  done.catch(() => {});
  return { stop: () => (running = false), done };
}

/** The environment without provider credentials, so an agent can reach only the mock (OpenCode alone takes up any
 * DEEPSEEK_API_KEY it finds). */
function withoutCredentials(env) {
  return Object.fromEntries(Object.entries(env).filter(([key]) => !/(_API_KEY|_AUTH_TOKEN|^GH_TOKEN|^GITHUB_TOKEN)$/u.test(key)));
}
