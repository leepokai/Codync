import { OpenAiLogo } from "@phosphor-icons/react/ssr";

// Agent brand marks: Simple Icons slugs, and OpenAI's mark from Phosphor for Codex.
const simpleIcons = { claude: "claude", gemini: "googlegemini", cursor: "cursor" } as const;

export type Agent = keyof typeof simpleIcons | "codex";

export default function AgentMark({ agent, className }: { agent: Agent; className: string }) {
  if (agent === "codex") return <OpenAiLogo weight="fill" className={`text-neutral-50 ${className}`} />;
  return (
    // eslint-disable-next-line @next/next/no-img-element -- brand marks from Simple Icons
    <img src={`https://cdn.simpleicons.org/${simpleIcons[agent]}/ffffff`} alt="" width={18} height={18} className={className} />
  );
}
