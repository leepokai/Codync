import type { Metadata } from "next";
import Link from "next/link";
import { BookOpen, Cube, FileText, GithubLogo, ShieldCheck, UserFocus } from "@phosphor-icons/react/ssr";
import CopyCommand from "../components/copy-command";
import Eyebrow from "../components/eyebrow";
import Halftone from "../components/halftone";
import Reveal from "../components/reveal";
import { SiteFooter, SiteHeader } from "../components/site-chrome";
import { CODYNC_MEM_CRATE, CODYNC_MEM_DOCS, CODYNC_MEM_REPO, GITHUB } from "../links";
import MemoryFiles from "./memory-files";

// The memory crate Codync's bots run on (host/crates/memory), for Rust developers building their own agents.
export const metadata: Metadata = {
  title: "codync-mem: memory for coding-agent bots, in Rust",
  description:
    "codync-mem is the open-source Rust crate behind Codync's bot memory: Markdown files the bot keeps itself, deterministic checks on every change, and rules for who may change what. Apache 2.0.",
  alternates: { canonical: "/codync-mem" },
};

const principles = [
  {
    icon: UserFocus,
    title: "It records the person, not the work",
    body: "What you prefer, how you work, what you decided. Project knowledge stays in AGENTS.md or CLAUDE.md, and the full conversation stays in your transcript.",
  },
  {
    icon: FileText,
    title: "Markdown files the bot keeps",
    body: "Each layer is a folder: MEMORY.md, one short fact per line that every session opens with, and notes on one topic each in topic folders (health/allergies.md), read when they come up. The bot edits them like any file.",
  },
  {
    icon: ShieldCheck,
    title: "Rules, not a model's judgment",
    body: "Every change a bot or dreaming makes passes deterministic checks, and the crate keeps every version and who wrote each fact, with the message it was learned from.",
  },
];

const rights: [who: string, may: string][] = [
  ["A turn you started in the bot's own chat", "The bot may change or take out anything."],
  ["Routines, group rooms, other bots", "Lines are only added, never changed or taken out."],
  ["Dreaming, between turns", "Adds and takes out lines, each quoting what you typed. It never takes out or rewrites a line you wrote, and makes at most 10 changes a run, taking out at most a quarter of the lines. Once MEMORY.md is nearly full it moves detail, yours too, word for word into topic notes, keeping one line per topic that points to its note."],
  ["Something you had it forget", "Stays out, unless you ask for it again yourself."],
];

const checks = ["Hidden characters", "Prompt-carrying links", "Planted instructions", "Passwords, keys, ID and card numbers", "Size caps"];

const modules: [name: string, does: string][] = [
  ["schema", "The crate's tables on your connection: migrate creates or updates them."],
  ["files", "A layer's files: versions, who wrote each fact, what a bot may change (admit), search and the notes' index."],
  ["check", "The deterministic checks every change but the user's own passes."],
  ["opening", "The memory a session opens with, rendered for its instructions."],
  ["dream", "A job for the user's own model, and the rules that audit its answer."],
  ["evidence", "What counts as the user's own words: not pasted, quoted or forwarded text."],
  ["compaction", "When to summarize a conversation ahead of time, the loop guard and the handoff."],
  ["store", "Tombstones of what the user had a bot forget."],
  ["tokens", "The estimate the caps count in: one per CJK character, one per four others."],
];

const USAGE = `use codync_mem::rusqlite::Connection;

let mut conn = Connection::open_in_memory()?;
codync_mem::schema::migrate(&conn)?;`;

const crateLinks = [
  [CODYNC_MEM_CRATE, Cube, "crates.io"],
  [CODYNC_MEM_DOCS, BookOpen, "docs.rs"],
  [CODYNC_MEM_REPO, GithubLogo, "GitHub"],
] as const;

const textLink = "text-neutral-200 underline underline-offset-4 hover:text-white";

export default function Page() {
  return (
    <>
      <SiteHeader />
      <main className="flex-1">
        <section className="relative overflow-hidden px-4 pt-16 pb-20 sm:px-6 md:pt-24">
          <Halftone corner="top-right" className="w-[36rem] max-w-[80vw] text-neutral-50 opacity-[0.14]" />
          <div className="relative mx-auto grid max-w-6xl grid-cols-1 items-center gap-12 md:grid-cols-[minmax(0,1.1fr)_minmax(0,1fr)] md:gap-16">
            <div>
              <span className="inline-flex items-center gap-2 rounded-full bg-neutral-900 py-1.5 pr-4 pl-1.5 text-sm">
                <span className="rounded-full bg-neutral-50 px-2.5 py-0.5 text-xs font-semibold text-neutral-950">Rust crate</span>
                <span className="text-neutral-300">Open source, from Codync</span>
              </span>
              <h1 className="mt-8 font-mono text-5xl font-semibold tracking-tighter text-neutral-50 md:text-6xl lg:text-7xl">codync-mem</h1>
              <p className="mt-6 max-w-[34rem] text-lg leading-relaxed text-neutral-400">
                Memory for coding-agent bots. The memory Codync&apos;s bots run on, as a standalone crate: Markdown files
                the bot keeps itself, and the rules for who may change them.
              </p>
              <div className="mt-9 max-w-[22rem] rounded-2xl bg-neutral-900 p-1.5">
                <CopyCommand command="cargo add codync-mem" />
              </div>
              <div className="mt-4 flex flex-wrap gap-3">
                {crateLinks.map(([href, Icon, label]) => (
                  <a
                    key={label}
                    href={href}
                    className="inline-flex h-11 items-center gap-2 rounded-full bg-neutral-900 px-5 text-sm font-medium text-neutral-100 transition hover:bg-neutral-800 active:scale-[0.98]"
                  >
                    <Icon size={18} />
                    {label}
                  </a>
                ))}
              </div>
            </div>
            <MemoryFiles />
          </div>
        </section>

        <section className="px-4 py-16 sm:px-6 md:py-20">
          <div className="mx-auto grid max-w-6xl grid-cols-1 gap-4 md:grid-cols-3">
            {principles.map(({ icon: Icon, title, body }, i) => (
              <Reveal key={title} delay={i * 0.05}>
                <div className="h-full rounded-3xl bg-neutral-900 p-8">
                  <Icon size={26} className="text-neutral-50" />
                  <h2 className="mt-5 text-xl font-semibold text-neutral-50">{title}</h2>
                  <p className="mt-3 leading-relaxed text-neutral-400">{body}</p>
                </div>
              </Reveal>
            ))}
          </div>
        </section>

        <section className="px-4 py-16 sm:px-6 md:py-20">
          <div className="mx-auto grid max-w-6xl grid-cols-1 gap-10 md:grid-cols-[1fr_1.4fr] md:gap-20">
            <Reveal>
              <Eyebrow>Who may change what</Eyebrow>
              <h2 className="text-3xl font-semibold tracking-tight text-neutral-50 md:text-4xl">Outside your own chat, memory only grows.</h2>
              <p className="mt-5 max-w-[26rem] text-lg leading-relaxed text-neutral-400">
                A routine, a group room or another bot can add a line, but only a turn you started can change or take
                one out.
              </p>
            </Reveal>
            <Reveal delay={0.05}>
              <dl className="divide-y divide-neutral-800">
                {rights.map(([who, may]) => (
                  <div key={who} className="grid grid-cols-1 gap-1 py-5 sm:grid-cols-[14rem_1fr] sm:gap-6">
                    <dt className="font-medium text-neutral-100">{who}</dt>
                    <dd className="leading-relaxed text-neutral-400">{may}</dd>
                  </div>
                ))}
              </dl>
            </Reveal>
          </div>
        </section>

        <section className="px-4 py-16 sm:px-6 md:py-20">
          <Reveal className="mx-auto max-w-6xl">
            <div className="relative overflow-hidden rounded-3xl bg-neutral-50 p-8 md:p-12">
              <Halftone className="w-80 text-neutral-950 opacity-[0.1]" />
              <Eyebrow>Checked before it lands</Eyebrow>
              <h2 className="relative max-w-[36rem] text-3xl font-semibold tracking-tight text-neutral-950 md:text-4xl">
                Deterministic checks, no model in the loop.
              </h2>
              <p className="relative mt-5 max-w-[36rem] text-lg leading-relaxed text-neutral-600">
                Whatever a change brings in is checked first, within caps of 150 tokens a line, 400 for yours and 1,200 per
                bot. The crate says why it refused, in words meant for the bot, so Codync can put the file back and the bot
                can correct itself.
              </p>
              <ul className="relative mt-8 flex flex-wrap gap-2">
                {checks.map((c) => (
                  <li key={c} className="rounded-full bg-neutral-950 px-4 py-2 text-sm text-neutral-100">
                    {c}
                  </li>
                ))}
              </ul>
            </div>
          </Reveal>
        </section>

        <section className="px-4 py-16 sm:px-6 md:py-20">
          <div className="mx-auto grid max-w-6xl grid-cols-1 gap-10 md:grid-cols-[minmax(0,1fr)_minmax(0,1.2fr)] md:gap-20">
            <Reveal>
              <Eyebrow>Use it</Eyebrow>
              <h2 className="text-3xl font-semibold tracking-tight text-neutral-50 md:text-4xl">The crate decides; you act.</h2>
              <p className="mt-5 max-w-[30rem] text-lg leading-relaxed text-neutral-400">
                It starts no processes, opens no network connections, calls no models and spawns no threads. It keeps its
                own tables on your SQLite connection and writes inside your transaction, so a memory change commits
                together with whatever you record alongside it.
              </p>
              <pre className="mt-8 overflow-x-auto rounded-xl bg-neutral-900 p-5 font-mono text-sm leading-relaxed text-neutral-200">
                <code>{USAGE}</code>
              </pre>
              <p className="mt-4 max-w-[30rem] text-sm leading-relaxed text-neutral-500">
                Use the <code className="font-mono text-neutral-300">rusqlite</code> it re-exports: a build can link only
                one to SQLite.
              </p>
            </Reveal>
            <Reveal delay={0.05}>
              <dl className="divide-y divide-neutral-800">
                {modules.map(([name, does]) => (
                  <div key={name} className="grid grid-cols-1 gap-1 py-4 sm:grid-cols-[8rem_1fr] sm:gap-6">
                    <dt className="font-mono text-sm text-neutral-100">{name}</dt>
                    <dd className="text-sm leading-relaxed text-neutral-400">{does}</dd>
                  </div>
                ))}
              </dl>
            </Reveal>
          </div>
        </section>

        <section className="px-4 pt-16 pb-24 sm:px-6 md:pb-32">
          <Reveal className="mx-auto max-w-6xl rounded-3xl bg-neutral-900 p-8 md:p-12">
            <h2 className="text-2xl font-semibold tracking-tight text-neutral-50 md:text-3xl">0.x, and it moves with Codync.</h2>
            <p className="mt-4 max-w-[40rem] leading-relaxed text-neutral-400">
              It&apos;s developed inside Codync and mirrored read-only, so the API changes with Codync. Report issues on{" "}
              <a href={CODYNC_MEM_REPO} className={textLink}>the mirror</a>, and send pull requests to{" "}
              <a href={`${GITHUB}/tree/dev/host/crates/memory`} className={textLink}>Codync&apos;s dev branch</a>{" "}
              (<code className="font-mono text-neutral-300">host/crates/memory</code>). Apache 2.0.
            </p>
            <Link href="/" className={`mt-6 inline-flex font-medium ${textLink}`}>
              See the bots it remembers for
            </Link>
          </Reveal>
        </section>
      </main>
      <SiteFooter />
    </>
  );
}
