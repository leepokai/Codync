import { DiscordLogo, Plus } from "@phosphor-icons/react/ssr";
import Reveal from "./reveal";
import { DISCORD } from "../links";

const questions: [string, string][] = [
  [
    "Is Codync free?",
    "Yes. Codync costs $0 with every feature included, and it's Apache 2.0 licensed. You pay only your agent's provider, through the plan or key you already have.",
  ],
  [
    "Can I use my Claude or ChatGPT plan?",
    "Yes. Codync runs the agents already signed in on your computer: Claude Code with your Claude plan, Codex with your ChatGPT plan, and so on. It never calls a provider's API with your login.",
  ],
  [
    "Which agents can it run?",
    "Claude Code, Codex, Cursor, Gemini, Copilot, OpenCode and about 40 more through the Agent Client Protocol registry. Each bot picks its own.",
  ],
  [
    "How do I start?",
    "Install Codync on the computer your agents run on and create a bot. Then pair your iPhone by scanning the code from the Mac menu bar, the Windows or Linux app, or codync-host pair.",
  ],
  [
    "Where does my data go?",
    "Bots, chats and memory stay on your computer. Your phone reaches it directly on the same network, or through an end-to-end encrypted relay anywhere else, which can't read your messages. Your agent's provider receives the prompts the agent sends.",
  ],
  [
    "Do I need an account?",
    "No. Pair your phone by scanning a code. Signing in with Apple or Google is optional: it lists every computer on your account on your phone, and each computer still approves each device itself.",
  ],
  [
    "Which computers can run it?",
    "A Mac, a Windows PC, or a Linux desktop or headless server. The iPhone app and the terminal UI over SSH talk to the same host.",
  ],
  [
    "Is it safe to let bots work on my computer?",
    "Bots act with your permissions. When an agent asks to edit files or run a command, the request comes to your phone to allow once, always allow or deny. Give bots work you trust, and keep backups.",
  ],
];

// The same questions as FAQPage structured data, so search and AI answers can quote them.
const faqJsonLd = {
  "@context": "https://schema.org",
  "@type": "FAQPage",
  mainEntity: questions.map(([q, a]) => ({
    "@type": "Question",
    name: q,
    acceptedAnswer: { "@type": "Answer", text: a },
  })),
};

export default function Questions() {
  return (
    <section id="faq" className="scroll-mt-20 px-4 py-20 sm:px-6 md:py-28">
      <script
        type="application/ld+json"
        dangerouslySetInnerHTML={{ __html: JSON.stringify(faqJsonLd).replace(/</g, "\\u003c") }}
      />
      <div className="mx-auto grid max-w-6xl grid-cols-1 gap-10 md:grid-cols-[1fr_1.4fr] md:gap-20">
        <Reveal>
          <h2 className="text-3xl font-semibold tracking-tight text-neutral-50 md:text-4xl">Questions</h2>
          <p className="mt-4 max-w-[22rem] text-lg leading-relaxed text-neutral-400">
            Short answers about price, setup, agents and your data.
          </p>
          <a
            href={DISCORD}
            className="mt-6 inline-flex items-center gap-2 font-medium text-neutral-200 hover:text-white"
          >
            <DiscordLogo size={20} />
            <span className="underline underline-offset-4">Ask on Discord</span>
          </a>
        </Reveal>
        <Reveal delay={0.05}>
          <div className="divide-y divide-neutral-800">
            {questions.map(([q, a]) => (
              <details key={q} className="group py-5">
                <summary className="flex cursor-pointer list-none items-center justify-between gap-6 text-lg font-medium text-neutral-100 [&::-webkit-details-marker]:hidden">
                  {q}
                  <Plus
                    size={18}
                    className="shrink-0 text-neutral-500 transition-transform duration-300 group-open:rotate-45"
                  />
                </summary>
                <p className="mt-3 max-w-[38rem] leading-relaxed text-neutral-400">{a}</p>
              </details>
            ))}
          </div>
        </Reveal>
      </div>
    </section>
  );
}
