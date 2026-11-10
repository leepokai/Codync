import { OpenAiLogo } from "@phosphor-icons/react/ssr";

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

/** `ink`: light marks for dark surfaces (the default), dark ones for light surfaces. */
export default function AgentMark({ agent, className, ink = "light" }: { agent: Agent; className: string; ink?: "light" | "dark" }) {
  if (agent === "codex") {
    return <OpenAiLogo weight="fill" className={`${ink === "light" ? "text-neutral-50" : "text-neutral-950"} ${className}`} />;
  }
  const color = ink === "light" ? "ffffff" : "0a0a0a";
  return (
    // eslint-disable-next-line @next/next/no-img-element -- brand marks from Simple Icons
    <img src={`https://cdn.simpleicons.org/${simpleIcons[agent]}/${color}`} alt="" width={18} height={18} className={className} />
  );
}
