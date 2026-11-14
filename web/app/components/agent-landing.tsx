import { BellRinging, ChartBar, ChatsCircle, ShieldCheck } from "@phosphor-icons/react/ssr";
import { AppleLogo } from "./apple-logo";
import Link from "next/link";
import Halftone from "./halftone";
import Phone from "./phone";
import { SiteFooter, SiteHeader } from "./site-chrome";
import type { AgentPage } from "../agents/agents";
import { AppStoreButton, PlayStoreButton } from "./store-buttons";
import { DMG } from "../links";

// One page per agent people want on their phone ("Claude Code app for iPhone").
export default function AgentLanding({ page }: { page: AgentPage }) {
  const { agent } = page;
  const points = [
    { icon: ChatsCircle, title: `${agent} as named bots`, body: `Give each ${agent} bot a name, a project folder and instructions. Each keeps one ongoing chat, and you see its final reply for every turn.` },
    { icon: ShieldCheck, title: "Approve from your phone", body: `When ${agent} wants to edit a file or run a command, the request comes to your iPhone: allow once, always allow or deny.` },
    { icon: BellRinging, title: "Pinged only when it matters", body: "A notification when a bot needs you or finishes, plus a Live Activity and the Dynamic Island while it works." },
    { icon: ChartBar, title: "Your limits at a glance", body: page.usage },
  ];
  const steps = [
    ["Install Codync on your computer", "On a Mac, download the app or run brew install --cask leepokai/codync/codync. On Windows, run the installer. On Linux, use the install script."],
    [`Sign in to ${agent} there`, page.login],
    [`Create a ${agent} bot`, `Pick ${agent} as the bot's agent and choose the folder it works in.`],
    ["Pair your iPhone", "Get Codync from the App Store and scan the pairing code from the Mac menu bar, or the Windows or Linux app."],
  ];
  const faqJsonLd = {
    "@context": "https://schema.org",
    "@type": "FAQPage",
    mainEntity: page.faq.map(([q, a]) => ({ "@type": "Question", name: q, acceptedAnswer: { "@type": "Answer", text: a } })),
  };

  return (
    <>
      <SiteHeader />
      <script type="application/ld+json" dangerouslySetInnerHTML={{ __html: JSON.stringify(faqJsonLd).replace(/</g, "\\u003c") }} />
      <main className="flex-1">
        <section className="relative overflow-hidden px-4 pt-16 pb-16 sm:px-6 md:pt-24">
          <Halftone corner="top-right" className="w-[36rem] max-w-[80vw] text-neutral-50 opacity-[0.14]" />
          <div className="relative mx-auto max-w-6xl">
            <h1 className="max-w-[44rem] text-4xl font-semibold tracking-tighter text-neutral-50 md:text-6xl">
              {agent} on your iPhone. 100% free.
            </h1>
            <p className="mt-6 max-w-[40rem] text-lg leading-relaxed text-neutral-400">{page.lead}</p>
            <div className="mt-9 flex flex-wrap gap-3">
              <a href={DMG} className="inline-flex h-12 items-center gap-2 rounded-full bg-neutral-50 px-6 font-medium text-neutral-950 transition hover:bg-white active:scale-[0.98]">
                <AppleLogo size={18} weight="fill" />
                Download for Mac
              </a>
              <AppStoreButton />
              <PlayStoreButton />
            </div>
          </div>
        </section>

        <section className="px-4 py-12 sm:px-6">
          <ul className="mx-auto grid max-w-6xl grid-cols-1 gap-4 md:grid-cols-2">
            {points.map(({ icon: Icon, title, body }) => (
              <li key={title} className="rounded-3xl bg-neutral-900 p-8">
                <Icon size={26} className="text-neutral-50" />
                <h2 className="mt-5 text-xl font-semibold text-neutral-50">{title}</h2>
                <p className="mt-3 leading-relaxed text-neutral-400">{body}</p>
              </li>
            ))}
          </ul>
        </section>

        <section className="px-4 py-16 sm:px-6">
          <div className="mx-auto grid max-w-6xl grid-cols-1 gap-12 md:grid-cols-[1fr_1fr] md:items-center">
            <div className="mx-auto w-full max-w-[17rem]">
              <Phone src="/screens/approval.webp" alt="An approval card on the iPhone: a bot asks to edit src/pace.js, with Allow once, Always allow and Deny" />
            </div>
            <div>
              <h2 className="text-3xl font-semibold tracking-tight text-neutral-50">Set it up in four steps</h2>
              <ol className="mt-8 space-y-6">
                {steps.map(([title, body], i) => (
                  <li key={title} className="flex gap-4">
                    <span className="flex size-8 shrink-0 items-center justify-center rounded-full bg-neutral-800 text-sm font-medium text-neutral-100">{i + 1}</span>
                    <div>
                      <p className="font-medium text-neutral-100">{title}</p>
                      <p className="mt-1 leading-relaxed text-neutral-400">{body}</p>
                    </div>
                  </li>
                ))}
              </ol>
            </div>
          </div>
        </section>

        <section className="px-4 py-16 sm:px-6">
          <div className="mx-auto grid max-w-6xl grid-cols-1 gap-10 md:grid-cols-[1fr_1.4fr] md:gap-20">
            <h2 className="text-3xl font-semibold tracking-tight text-neutral-50">Questions</h2>
            <div className="divide-y divide-neutral-800">
              {page.faq.map(([q, a]) => (
                <div key={q} className="py-5">
                  <h3 className="text-lg font-medium text-neutral-100">{q}</h3>
                  <p className="mt-2 leading-relaxed text-neutral-400">{a}</p>
                </div>
              ))}
            </div>
          </div>
        </section>

        <section className="px-4 pt-8 pb-24 sm:px-6">
          <p className="mx-auto max-w-6xl text-neutral-400">
            Using more than {agent}? Codync runs Claude Code, Codex, Cursor, Gemini, Copilot and about 40 more agents.{" "}
            <Link href="/" className="text-neutral-200 underline underline-offset-4 hover:text-white">
              See everything Codync does
            </Link>
            {" "}or{" "}
            <Link href="/compare" className="text-neutral-200 underline underline-offset-4 hover:text-white">
              compare it with other apps
            </Link>
            .
          </p>
        </section>
      </main>
      <SiteFooter />
    </>
  );
}
