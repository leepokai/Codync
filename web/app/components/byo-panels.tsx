import { ArrowBendDownRight, Cloud, HardDrives, Laptop } from "@phosphor-icons/react/ssr";
import AgentMark, { agentName, agents, type Agent } from "./agent-mark";
import Halftone from "./halftone";
import { BotRow, ComputerLabel } from "./sidebar-rows";

// The pictures in "Bring your own sandbox": drawn like the apps' own sidebar, agent list, Agent card and cost.

const sandboxes = [
  {
    icon: Laptop,
    name: "MacBook Pro",
    where: "This Mac",
    bots: [
      { name: "Scout", shape: "cloud", color: "green", line: "Added weeklyTotals with a test" },
      { name: "Reviewer", shape: "tablet", color: "violet", line: "Two issues in src/pace.js" },
    ],
  },
  {
    icon: HardDrives,
    name: "build-box",
    where: "Over SSH",
    bots: [{ name: "Pacer", shape: "hex", color: "orange", line: "Running the test suite…" }],
  },
  {
    icon: Cloud,
    name: "Research",
    where: "Codync Cloud, sleeping",
    bots: [{ name: "Relay", shape: "squircle", color: "blue", line: "Weekly report is ready" }],
  },
];

/** Each computer drawn as a sandbox: a dashed boundary around its bots. */
export function SandboxPanel() {
  return (
    <div
      role="img"
      aria-label="Three sandboxes: a MacBook Pro with Scout and Reviewer, build-box over SSH with Pacer, and Research, a sleeping Codync Cloud computer, with Relay"
      className="flex flex-col gap-3"
    >
      {sandboxes.map(({ icon, name, where, bots }) => (
        <div key={name} className="rounded-3xl border-2 border-dashed border-neutral-800 p-3 pt-4">
          <ComputerLabel icon={icon} name={name} detail={where} />
          <ul className="mt-2.5 flex flex-col gap-1">
            {bots.map((bot) => (
              <li key={bot.name}>
                <BotRow {...bot} />
              </li>
            ))}
          </ul>
        </div>
      ))}
    </div>
  );
}

// The bot's Agent card while Codex carries on for Claude Code (docs/features/agent-fallback.md).
const chain: { agent: Agent; resets?: string; inUse?: boolean }[] = [
  { agent: "claude", resets: "3:00 PM" },
  { agent: "codex", inUse: true },
  { agent: "gemini" },
];

export function AgentPanel() {
  return (
    <div
      role="img"
      aria-label="A bot's agents: Claude Code, its usage resetting at 3:00 PM; Codex, in use; Gemini CLI. The chat says Codex carried on."
      className="p-2"
    >
      <ol className="flex flex-col gap-2">
        {chain.map(({ agent, resets, inUse }) => (
          <li key={agent} className="flex items-center gap-3 rounded-2xl bg-neutral-900 px-4 py-3.5">
            <AgentMark agent={agent} className="size-[18px]" />
            <span className="flex min-w-0 flex-1 flex-col">
              <span className={`truncate font-medium ${resets ? "text-neutral-400" : "text-neutral-50"}`}>{agentName[agent]}</span>
              {resets ? <span className="text-xs text-neutral-500">Usage resets {resets}</span> : null}
            </span>
            {inUse ? (
              <span className="rounded-full bg-neutral-50 px-2.5 py-0.5 text-xs font-semibold text-neutral-950">In use</span>
            ) : null}
          </li>
        ))}
      </ol>
      <p className="mt-6 flex items-start gap-2 text-sm leading-relaxed text-neutral-400">
        <ArrowBendDownRight size={16} className="mt-0.5 shrink-0" />
        Claude Code reached its usage limit (resets in 2 h). Codex carried on.
      </p>
      <p className="mt-3 max-w-[85%] rounded-2xl bg-neutral-900 px-4 py-3 text-sm leading-relaxed text-neutral-200">
        Done: the date guard is in, and all 5 tests pass.
      </p>
    </div>
  );
}

// Who gets paid for what: Codync for nothing, the providers through the plans you have.
const bills: { agent?: Agent; name: string; paid: string }[] = [
  { agent: "claude", name: "Claude Code", paid: "Your Claude plan" },
  { agent: "codex", name: "Codex", paid: "Your ChatGPT plan" },
  { name: "Codync Cloud", paid: "Your Boat or InstaBox account" },
];

/** Drawn dark on light: it sits in a light well. */
export function PlanPanel() {
  return (
    <div className="relative text-neutral-950">
      <Halftone className="w-56 text-neutral-950 opacity-[0.1]" />
      <p className="relative text-7xl leading-none font-semibold tracking-tighter">$0</p>
      <p className="relative mt-2 text-sm text-neutral-600">for Codync, with every feature included.</p>
      <ul className="relative mt-6 flex flex-col gap-1">
        {bills.map(({ agent, name, paid }) => (
          <li key={name} className="flex items-center gap-3 rounded-2xl bg-neutral-200/70 px-3 py-2.5">
            {agent ? <AgentMark agent={agent} ink="dark" className="size-4 shrink-0" /> : <Cloud size={16} weight="fill" className="shrink-0" />}
            <span className="flex min-w-0 flex-col">
              <span className="text-sm font-medium">{name}</span>
              <span className="truncate text-xs text-neutral-600">{paid}</span>
            </span>
          </li>
        ))}
      </ul>
    </div>
  );
}

/** The agents a bot can pick, as chips. */
export function AgentsPanel() {
  return (
    <ul className="flex flex-wrap gap-2">
      {agents.map((agent) => (
        <li key={agent} className="inline-flex items-center gap-2 rounded-full bg-neutral-900 py-2 pr-4 pl-3 text-sm text-neutral-200">
          <AgentMark agent={agent} className="size-4" />
          {agentName[agent]}
        </li>
      ))}
      <li className="rounded-full px-3 py-2 text-sm text-neutral-500">and 40 more</li>
    </ul>
  );
}
