import { Check, Plus } from "@phosphor-icons/react/ssr";
import Eyebrow from "./eyebrow";
import Halftone from "./halftone";
import Reveal from "./reveal";
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

export default function Compare() {
  return (
    <section id="compare" className="px-4 py-20 sm:px-6 md:py-28">
      <div className="mx-auto max-w-6xl">
        <Reveal>
          <Eyebrow>Compare</Eyebrow>
          <h2 className="max-w-[40rem] text-3xl font-semibold tracking-tight text-neutral-50 md:text-4xl">
            Everything Grok Bot and Muse do. Open source, with any agent.
          </h2>
          <p className="mt-5 max-w-[36rem] text-lg leading-relaxed text-neutral-400">
            Codync is a 1:1 alternative: the same bot-based way of working, rebuilt in the open so it isn&apos;t tied
            to one model or one subscription.
          </p>
        </Reveal>

        <div className="mt-12 grid grid-cols-1 gap-4 md:grid-cols-2">
          <Reveal>
            <div className="h-full rounded-3xl bg-neutral-900 p-8 md:p-10">
              <h3 className="text-xl font-semibold text-neutral-50">Same features</h3>
              <ul className="mt-6 space-y-3">
                {same.map((s) => (
                  <li key={s} className="flex gap-3 leading-relaxed text-neutral-300">
                    <Check size={20} weight="bold" className="mt-0.5 shrink-0 text-neutral-50" />
                    {s}
                  </li>
                ))}
              </ul>
            </div>
          </Reveal>

          <Reveal delay={0.05}>
            <div className="relative h-full overflow-hidden rounded-3xl bg-neutral-50 p-8 md:p-10">
              <Halftone className="w-72 text-neutral-950 opacity-[0.12]" />
              <h3 className="relative text-xl font-semibold text-neutral-950">What&apos;s different</h3>
              <ul className="relative mt-6 space-y-6">
                {different.map(([title, body]) => (
                  <li key={title} className="flex gap-3">
                    <Plus size={20} weight="bold" className="mt-0.5 shrink-0 text-neutral-950" />
                    <div>
                      <p className="font-medium text-neutral-950">{title}</p>
                      <p className="mt-1 leading-relaxed text-neutral-600">{body}</p>
                    </div>
                  </li>
                ))}
              </ul>
              <a href={GITHUB} className="relative mt-8 inline-block font-medium text-neutral-950 underline underline-offset-4 hover:text-black">
                Read the code on GitHub
              </a>
            </div>
          </Reveal>
        </div>

        <p className="mt-6 text-sm text-neutral-600">
          Grok Bot and Muse are products of their respective owners. Codync is an independent project and isn&apos;t
          affiliated with them.
        </p>
      </div>
    </section>
  );
}
