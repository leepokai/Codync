import { CalendarCheck, ChatsCircle, Code, Plugs, WindowsLogo } from "@phosphor-icons/react/ssr";
import { AppleLogo } from "../components/apple-logo";
import type { Metadata } from "next";
import Cost from "../components/cost";
import Halftone from "../components/halftone";
import Install from "../components/install";
import { SiteFooter, SiteHeader } from "../components/site-chrome";
import Surfaces from "../components/surfaces";
import { AppStoreButton, PlayStoreButton } from "../components/store-buttons";
import { DMG, WINDOWS } from "../links";

// For people searching "free AI agent" / "AI agent app": what free means here, and what the bots do.
export const metadata: Metadata = {
  title: "Free AI agent app for iPhone, Mac, Windows and Linux",
  description:
    "Codync is a 100% free, open-source AI agent app. Run Claude Code, Codex, Cursor, Gemini and 40+ agents as named bots on your own computer, for code and everyday tasks, and message them from your iPhone. No subscription.",
  alternates: { canonical: "/free-ai-agent-app" },
};

const uses = [
  { icon: Code, title: "Code", body: "Fix a bug, review a pull request or build a feature. Approve edits and commands from your phone." },
  { icon: Plugs, title: "Your apps", body: "Connect email, calendars, docs and more through the MCP registry and Composio, and let bots browse the web." },
  { icon: CalendarCheck, title: "On a schedule", body: "Routines run a bot every morning, every hour or from a webhook, and you get the result as a message." },
  { icon: ChatsCircle, title: "As a team", body: "Put several bots in a group chat, mix Claude Code and Codex, and let them ask each other for help." },
];

const faq: [string, string][] = [
  [
    "Is there a free AI agent app?",
    "Codync is one: the app and every feature cost $0 and it's open source (Apache 2.0). Your bots use the agent plans or API keys you already have, such as a Claude or ChatGPT plan.",
  ],
  [
    "What's the catch?",
    "None from Codync: no paid tier, no ads, no locked features. The agents themselves still need their own provider plan or key, and they run on your computer.",
  ],
  [
    "Which AI agents can I use?",
    "Claude Code, Codex, Cursor, Gemini, Copilot, OpenCode and about 40 more through the Agent Client Protocol registry. Each bot picks its own.",
  ],
  [
    "Does it work on my phone?",
    "Yes. The agents run on your Mac, Windows PC or Linux machine, and the free iPhone app lets you message them, approve their actions and get notified from anywhere.",
  ],
];

const faqJsonLd = {
  "@context": "https://schema.org",
  "@type": "FAQPage",
  mainEntity: faq.map(([q, a]) => ({ "@type": "Question", name: q, acceptedAnswer: { "@type": "Answer", text: a } })),
};

export default function Page() {
  return (
    <>
      <SiteHeader />
      <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(faqJsonLd).replace(/</g, "\\u003c") }} />
      <main className="flex-1">
        <section className="relative overflow-hidden px-4 pt-16 pb-16 sm:px-6 md:pt-24">
          <Halftone corner="top-right" className="w-[36rem] max-w-[80vw] text-neutral-50 opacity-[0.14]" />
          <div className="relative mx-auto max-w-6xl">
            <h1 className="max-w-[48rem] text-4xl font-semibold tracking-tighter text-neutral-50 md:text-6xl">
              The free AI agent app. Really free.
            </h1>
            <p className="mt-6 max-w-[40rem] text-lg leading-relaxed text-neutral-400">
              Codync turns Claude Code, Codex, Cursor, Gemini and 40+ AI agents into named bots on your own computer, for
              code and everyday tasks. Message them from your iPhone, Mac, Windows PC or Linux desktop. No subscription, no
              paid tier: it&apos;s open source.
            </p>
            <div className="mt-9 flex flex-wrap gap-3">
              <a href={DMG} className="inline-flex h-12 items-center gap-2 rounded-full bg-neutral-50 px-6 font-medium text-neutral-950 transition hover:bg-white active:scale-[0.98]">
                <AppleLogo size={18} weight="fill" />
                Download for Mac
              </a>
              <a href={WINDOWS} className="inline-flex h-12 items-center gap-2 rounded-full bg-neutral-900 px-6 font-medium text-neutral-100 transition hover:bg-neutral-800 active:scale-[0.98]">
                <WindowsLogo size={18} weight="fill" />
                Windows
              </a>
              <AppStoreButton />
              <PlayStoreButton />
            </div>
          </div>
        </section>

        <section className="px-4 py-12 sm:px-6">
          <ul className="mx-auto grid max-w-6xl grid-cols-1 gap-4 md:grid-cols-2">
            {uses.map(({ icon: Icon, title, body }) => (
              <li key={title} className="rounded-3xl bg-neutral-900 p-8">
                <Icon size={26} className="text-neutral-50" />
                <h2 className="mt-5 text-xl font-semibold text-neutral-50">{title}</h2>
                <p className="mt-3 leading-relaxed text-neutral-400">{body}</p>
              </li>
            ))}
          </ul>
        </section>

        <Cost />
        <Surfaces />

        <section className="px-4 py-16 sm:px-6">
          <div className="mx-auto grid max-w-6xl grid-cols-1 gap-10 md:grid-cols-[1fr_1.4fr] md:gap-20">
            <h2 className="text-3xl font-semibold tracking-tight text-neutral-50">Questions</h2>
            <div className="divide-y divide-neutral-800">
              {faq.map(([q, a]) => (
                <div key={q} className="py-5">
                  <h3 className="text-lg font-medium text-neutral-100">{q}</h3>
                  <p className="mt-2 leading-relaxed text-neutral-400">{a}</p>
                </div>
              ))}
            </div>
          </div>
        </section>

        <Install />
      </main>
      <SiteFooter />
    </>
  );
}
