// The processes a check leaves behind: the host's tree once it's stopped, and daemons an agent detached on purpose.

import { execFile } from "node:child_process";
import { promisify } from "node:util";

const run = promisify(execFile);
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));
// `ps` prints whole command lines (and, for `stopDaemons`, environments): far past execFile's 1 MiB default on a busy computer.
const PS = { maxBuffer: 64 * 1024 * 1024 };
// Daemons an agent detaches on purpose and shares with its other clients, listed on its docs/upstream-agents page:
// they may outlive the host (stopDaemons ends them after the check).
const DETACHED = /\bgateway run\b.*--idle-exit/u;

/** Ends what an agent detached on purpose to outlive the host (Hermes's gateway), found by the check's own HOME. */
export async function stopDaemons(home) {
  const { stdout } = await run("ps", ["axeww", "-o", "pid=,command="], PS);
  for (const line of stdout.split("\n").filter((l) => l.includes(` HOME=${home} `))) {
    try {
      process.kill(Number(line.trim().split(/\s+/u)[0]), "SIGKILL");
    } catch (error) {
      // Ended by itself between ps and kill.
      if (error.code !== "ESRCH") throw error;
    }
  }
}

/** Stops the host and returns its descendants still running 10 s later, if any. */
export async function stopHost(host) {
  const tree = descendants(await processTable(), host.pid);
  host.kill("SIGTERM");
  await new Promise((resolve) => (host.exitCode === null ? host.once("exit", resolve) : resolve()));
  await sleep(10_000);
  // Compared by command line too, so a pid reused since reads as gone.
  const alive = new Map((await processTable()).map((p) => [p.pid, p.command]));
  return tree
    .filter((p) => alive.get(p.pid) === p.command && !DETACHED.test(p.command))
    .map((p) => `${p.pid} ${p.command.slice(0, 200)}`)
    .join("; ");
}

async function processTable() {
  const { stdout } = await run("ps", ["-eo", "pid=,ppid=,command="], PS);
  return stdout.split("\n").filter(Boolean).map((line) => {
    const [pid, ppid, ...command] = line.trim().split(/\s+/u);
    return { pid: Number(pid), ppid: Number(ppid), command: command.join(" ") };
  });
}

/** Every process under `root` in `table`. */
function descendants(table, root) {
  const found = [];
  for (let parents = new Set([root]); parents.size; ) {
    const children = table.filter((p) => parents.has(p.ppid));
    found.push(...children);
    parents = new Set(children.map((c) => c.pid));
  }
  return found;
}
