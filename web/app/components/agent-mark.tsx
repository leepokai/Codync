import { OpenAiLogo } from "@phosphor-icons/react/ssr";
import { preload } from "react-dom";

// Agent brand marks: Simple Icons slugs, and OpenAI's mark from Phosphor for Codex.
const simpleIcons = {
  claude: "claude",
  gemini: "googlegemini",
  cursor: "cursor",
  copilot: "githubcopilot",
  opencode: "opencode",
} as const;

export type Agent = keyof typeof simpleIcons | "codex";

export const agentName: Record<Agent, string> = {
  claude: "Claude Code",
  codex: "Codex",
  gemini: "Gemini CLI",
  cursor: "Cursor",
  copilot: "Copilot",
  opencode: "OpenCode",
};

const src = (agent: keyof typeof simpleIcons) => `https://cdn.simpleicons.org/${simpleIcons[agent]}/ffffff`;

/** Fetches the marks up front, for marks that only mount later (the hero's rotating agent). */
export function preloadAgentMarks(agents: Agent[]) {
  for (const agent of agents) if (agent !== "codex") preload(src(agent), { as: "image" });
}

export default function AgentMark({ agent, className }: { agent: Agent; className: string }) {
  if (agent === "codex") return <OpenAiLogo weight="fill" className={`text-neutral-50 ${className}`} />;
  return (
    // eslint-disable-next-line @next/next/no-img-element -- brand marks from Simple Icons
    <img src={src(agent)} alt="" width={18} height={18} className={className} />
  );
}
