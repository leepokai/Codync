import { ArrowBendDownRight, ArrowsClockwise, ChatCircleText, UsersThree } from "@phosphor-icons/react/ssr";
import AgentMark, { agentName, type Agent } from "./agent-mark";
import Eyebrow from "./eyebrow";
import Halftone from "./halftone";
import Points, { type Point } from "./points";
import Reveal from "./reveal";

// The bot's Agent card while Codex carries on for Claude Code (docs/features/agent-fallback.md).
const chain: { agent: Agent; resets?: string; inUse?: boolean }[] = [
  { agent: "claude", resets: "3:00 PM" },
  { agent: "codex", inUse: true },
  { agent: "gemini" },
];

const points: Point[] = [
  {
    icon: ArrowsClockwise,
    title: "Picks up mid-turn",
    body: "The next agent hears what the turn already did and checks before it redoes a step. The bot goes back to its own agent once that one resets.",
  },
  {
    icon: UsersThree,
    title: "One rest for every bot",
    body: "Once one bot finds Claude Code out of usage, every bot skips it until it resets, instead of each one retrying.",
  },
  {
    icon: ChatCircleText,
    title: "Always says who answered",
    body: "A line in the chat marks each switch and the way back. Up to three fallbacks per bot, set in any app or by the bot itself.",
  },
];

export default function Fallbacks() {
  return (
    <section className="px-4 py-20 sm:px-6 md:py-28">
      <div className="mx-auto grid max-w-6xl grid-cols-1 items-center gap-12 md:grid-cols-[1.1fr_1fr] md:gap-20">
        <Reveal>
          <Eyebrow>Fallbacks</Eyebrow>
          <h2 className="text-3xl font-semibold tracking-tight text-neutral-50 md:text-4xl">
            Out of usage? Another agent carries on.
          </h2>
          <p className="mt-5 max-w-[32rem] text-lg leading-relaxed text-neutral-400">
            When a bot&apos;s agent hits its usage limit, gets rate limited, is signed out or won&apos;t start, the
            next agent in its list takes the same turn, in the same chat, with the same memory, folder and tools.
          </p>
          <Points points={points} />
        </Reveal>

        <Reveal delay={0.05}>
          <div
            role="img"
            aria-label="A bot's agents: Claude Code, its usage resetting at 3:00 PM; Codex, in use; Gemini CLI. The chat says Codex carried on."
            className="relative overflow-hidden rounded-3xl bg-neutral-900 p-6 md:p-8"
          >
            <Halftone className="w-80 text-neutral-50 opacity-[0.1]" />
            <ol className="relative flex flex-col gap-2">
              {chain.map(({ agent, resets, inUse }, i) => (
                <li key={agent} className="flex items-center gap-3 rounded-2xl bg-neutral-800 px-4 py-3.5">
                  <span className="w-4 text-sm text-neutral-500 tabular-nums">{i === 0 ? "" : i}</span>
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
            <p className="relative mt-6 flex items-start gap-2 text-sm leading-relaxed text-neutral-400">
              <ArrowBendDownRight size={16} className="mt-0.5 shrink-0" />
              Claude Code reached its usage limit (resets in 2 h). Codex carried on.
            </p>
            <p className="relative mt-3 max-w-[85%] rounded-2xl bg-neutral-800 px-4 py-3 text-sm leading-relaxed text-neutral-200">
              Done: the date guard is in, and all 5 tests pass.
            </p>
          </div>
        </Reveal>
      </div>
    </section>
  );
}
