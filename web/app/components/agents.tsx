import AgentMark, { agentName, type Agent } from "./agent-mark";

const agents: Agent[] = ["claude", "codex", "gemini", "cursor", "copilot", "opencode"];

// The agents a bot can run, straight under the hero.
export default function Agents() {
  return (
    <section aria-label="Supported agents" className="px-4 pb-20 sm:px-6 md:pb-28">
      <div className="mx-auto flex max-w-6xl flex-col gap-6 md:flex-row md:items-center md:gap-12">
        <p className="shrink-0 text-sm text-neutral-500">Runs the agents you already use</p>
        <ul className="flex flex-wrap items-center gap-x-8 gap-y-4">
          {agents.map((agent) => (
            <li key={agent} className="inline-flex items-center gap-2.5 font-medium text-neutral-300">
              <AgentMark agent={agent} className="size-5" />
              {agentName[agent]}
            </li>
          ))}
          <li className="text-neutral-500">and 40 more</li>
        </ul>
      </div>
    </section>
  );
}
