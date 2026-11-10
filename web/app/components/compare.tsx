import { Check, Plus } from "@phosphor-icons/react/ssr";
import CompareCards from "./compare-cards";
import Halftone from "./halftone";
import Reveal from "./reveal";
import SectionHeader from "./section-header";
import { GITHUB } from "../links";

const same = [
  "Persistent, named bots with their own instructions and project",
  "Group chats where bots answer in turn and read each other",
  "Bots asking each other for help",
  "Approval cards: allow once, always allow, deny",
  "Per-bot memory that carries across sessions",
  "Routines on a schedule or a webhook",
  "Remote screen: watch and control the computer from the phone",
  "Voice calls with a bot",
  "“Needs you” and “done” notifications",
];

const different = [
  ["100% free, no hidden fees", "Apache 2.0 licensed. No subscription, no paid tier, no in-app purchases."],
  ["Any coding agent, over ACP", "Speaks the Agent Client Protocol, so Claude Code, Codex, Cursor, Gemini, Copilot and 40+ more just work, with the logins you already have. Mix them in one room."],
  ["Built in Rust", "One small, fast binary hosts every bot. Bots, transcripts and memory stay on your computer; the phone reaches it end-to-end encrypted."],
  ["Mac, Windows and Linux", "The same host runs on macOS, Windows and Linux (a static binary on any distro, desktop or headless server). All three get the same desktop app, with a menu bar or tray icon and a chat window."],
  ["On every screen you have", "A native SwiftUI app on iPhone, a desktop app on Mac, Windows and Linux, and a terminal UI over SSH."],
];

// The features Codync shares with Grok Bot and Muse as chips, what sets it apart as a list, then one card per comparison page.
export default function Compare() {
  return (
    <section id="compare" className="scroll-mt-16 px-4 py-24 sm:px-6 md:py-32">
      <div className="mx-auto max-w-6xl">
        <SectionHeader label="Compare" title="Everything Grok Bot and Muse do. Open source, with any agent." />
        <Reveal>
          <ul className="mt-12 flex flex-wrap justify-center gap-2">
            {same.map((s) => (
              <li key={s} className="inline-flex items-center gap-2 rounded-full bg-neutral-900 py-2 pr-4 pl-3 text-sm text-neutral-300">
                <Check size={14} weight="bold" className="shrink-0 text-neutral-50" />
                {s}
              </li>
            ))}
          </ul>
        </Reveal>

        <Reveal>
          <div className="relative mt-4 overflow-hidden rounded-3xl bg-neutral-50 p-8 md:p-10">
            <Halftone className="w-80 text-neutral-950 opacity-[0.1]" />
            <h3 className="relative text-2xl font-semibold tracking-tight text-neutral-950">What&apos;s different</h3>
            <ul className="relative mt-8 grid grid-cols-1 gap-x-12 gap-y-8 md:grid-cols-2 lg:grid-cols-3">
              {different.map(([title, body]) => (
                <li key={title}>
                  <p className="flex items-center gap-2 font-medium text-neutral-950">
                    <Plus size={16} weight="bold" className="shrink-0" />
                    {title}
                  </p>
                  <p className="mt-2 leading-relaxed text-neutral-600">{body}</p>
                </li>
              ))}
              <li className="flex items-end">
                <a href={GITHUB} className="font-medium text-neutral-950 underline underline-offset-4 hover:text-black">
                  Read the code on GitHub
                </a>
              </li>
            </ul>
          </div>
        </Reveal>

        <Reveal className="mt-16">
          <h3 className="mb-6 text-xl font-semibold text-neutral-50">Compare Codync</h3>
          <CompareCards />
          <p className="mt-6 text-sm text-neutral-600">
            Grok Bot and Muse are products of their respective owners. Codync is an independent project and isn&apos;t
            affiliated with them.
          </p>
        </Reveal>
      </div>
    </section>
  );
}
