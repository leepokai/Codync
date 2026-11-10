import { AgentPanel, PlanPanel, SandboxPanel } from "./byo-panels";
import ByoScroller from "./byo-scroller";
import Reveal from "./reveal";

const steps = [
  {
    word: "sandbox.",
    body: "Your Mac or PC, a Linux server over SSH, or a cloud computer on your own Boat or InstaBox account. Bots, chats and memory stay there.",
    panel: <SandboxPanel />,
  },
  {
    word: "agent.",
    body: "Claude Code, Codex, Gemini, Cursor, Copilot, OpenCode or 40 more. Each bot picks its own, and another carries on when one runs out of usage.",
    panel: <AgentPanel />,
  },
  {
    word: "subscription.",
    id: "cost",
    body: "Bots run on the Claude, ChatGPT or other plans you already pay for. Codync itself is free, with no paid tier.",
    panel: <PlanPanel />,
  },
];

// Bring your own sandbox, agent and subscription, one step at a time: rendered here on the server; the scroller only tracks which step is in view.
// On phones each step carries its own heading.
export default function BringYourOwn() {
  return (
    <section className="px-4 sm:px-6">
      <h2 className="sr-only">Bring your own sandbox, agent and subscription</h2>
      <ByoScroller
        steps={steps.map(({ word, id, body, panel }) => ({
          word,
          id,
          content: (
            <Reveal>
              <h3 className="mb-8 text-4xl leading-[1.08] font-semibold tracking-tighter text-neutral-50 md:hidden">
                Bring your own {word}
              </h3>
              {panel}
              <p className="mt-6 max-w-[30rem] text-lg leading-relaxed text-neutral-400">{body}</p>
            </Reveal>
          ),
        }))}
      />
    </section>
  );
}
