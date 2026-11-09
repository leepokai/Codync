"use client";

import { useEffect, useRef, useState } from "react";
import { AnimatePresence, motion, useReducedMotion } from "framer-motion";
import {
  ArrowUp,
  CaretDown,
  CaretLeft,
  CaretRight,
  Chats,
  CardsThree,
  Desktop,
  Plus,
  Stop,
  User,
  Waveform,
  WifiHigh,
} from "@phosphor-icons/react";
import { AvatarWithStatus, CharacterAvatar, GroupAvatar, type Face, type Mood } from "./avatar";
import { clock, palette, paceDiff, swatch, type Chat, type Entry, type Permission } from "./data";

// The iPhone app's two screens at their real size in points (402 x 874), from
// CodyncUI/Bots/BotRow.swift, Thread/{ChatRows,PermissionCard,Composer}.swift and Chrome.swift.

export const SCREEN = { w: 402, h: 874 };
const mono = 'ui-monospace, "SF Mono", SFMono-Regular, Menlo, monospace';
const glass = "bg-white shadow-[0_0_0_0.5px_rgba(0,0,0,0.1),0_6px_18px_rgba(0,0,0,0.07)]";
const focus = "outline-none focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-[#1084FE]";
const press = "transition-transform duration-[50ms] active:scale-[0.98]";
/** Motion.conversation: a critically damped .42 s spring. */
const conversation = { type: "spring", duration: 0.42, bounce: 0 } as const;

export const moodOf = (c: Chat): Mood => (c.activity?.needs ? "needs" : c.activity ? "working" : "idle");
export const faceOf = (c: Chat): Face => ({ shape: c.shape, color: c.color, mood: moodOf(c) });

/** Inline Markdown the demo needs: `code` spans. */
function Inline({ text }: { text: string }) {
  return (
    <>
      {text.split("`").map((part, i) =>
        i % 2 ? (
          <code key={i} style={{ fontFamily: mono, fontSize: "0.94em" }}>
            {part}
          </code>
        ) : (
          part
        ),
      )}
    </>
  );
}

/** Stand-in for ThinkingOrb: a small turning ring of dots in the given ink. */
function Orb({ color, size = 16 }: { color: string; size?: number }) {
  const dots = Array.from({ length: 10 }, (_, i) => {
    const a = (i / 10) * 2 * Math.PI;
    return { x: (8 + Math.cos(a) * 6).toFixed(3), y: (8 + Math.sin(a) * 6).toFixed(3), o: (0.25 + 0.75 * (i / 9)).toFixed(3) };
  });
  return (
    <svg width={size} height={size} viewBox="0 0 16 16" aria-hidden className="shrink-0 animate-[spin_1.4s_linear_infinite] motion-reduce:animate-none">
      <circle cx={8} cy={8} r={1.6} fill={color} fillOpacity={0.6} />
      {dots.map((d, i) => (
        <circle key={i} cx={d.x} cy={d.y} r={1.15} fill={color} fillOpacity={d.o} />
      ))}
    </svg>
  );
}

export function StatusBar() {
  return (
    <div aria-hidden className="absolute inset-x-0 top-0 z-30 flex h-[54px] items-center justify-between pr-[38px] pl-[62px] pt-[10px]">
      <span className="text-[17px] font-semibold tracking-[-0.02em]" style={{ color: palette.text }}>
        9:41
      </span>
      <span className="flex items-center gap-[7px]">
        <svg width="19" height="12" viewBox="0 0 19 12">
          {[0, 1, 2, 3].map((i) => (
            <rect key={i} x={i * 5} y={9 - i * 3} width="3.2" height={3 + i * 3} rx="1" fill={palette.text} />
          ))}
        </svg>
        <WifiHigh size={18} weight="bold" color={palette.text} />
        <svg width="27" height="13" viewBox="0 0 27 13">
          <rect x="0.5" y="0.5" width="23" height="12" rx="3.8" fill="#34C759" />
          <rect x="24.5" y="4" width="1.8" height="5" rx="0.9" fill="#34C759" fillOpacity="0.5" />
          <path d="M13 2.2 8.6 7h3.2l-1 3.8L15.2 6H12z" fill="#fff" />
        </svg>
      </span>
    </div>
  );
}

function HomeIndicator() {
  return <div aria-hidden className="absolute bottom-[8px] left-1/2 z-30 h-[5px] w-[139px] -translate-x-1/2 rounded-full bg-black" />;
}

/* ----------------------------------------------------------------- Roster */

function preview(chat: Chat, chats: Chat[]) {
  if (chat.activity?.needs)
    return (
      <span className="flex min-w-0 items-center gap-[6px]" style={{ color: palette.warning }}>
        <Orb color={palette.warning} />
        <span className="truncate">{chat.activity.text}</span>
      </span>
    );
  if (chat.activity)
    return (
      <span className="flex min-w-0 items-center gap-[6px]" style={{ color: palette.secondary }}>
        <Orb color={palette.secondary} size={13} />
        <span className="truncate">{chat.activity.text}</span>
      </span>
    );
  const last = [...chat.entries].reverse().find((e) => e.kind === "agent" || e.kind === "user");
  const text = last && "text" in last ? last.text : "";
  const author = last?.kind === "agent" && last.author ? chats.find((c) => c.id === last.author)?.name : undefined;
  return (
    <span className="truncate" style={{ color: palette.secondary }}>
      {author ? `${author}: ${text}` : text}
    </span>
  );
}

export function Roster({ chats, open }: { chats: Chat[]; open: (id: string) => void }) {
  const sorted = [...chats].sort((a, b) => b.at - a.at);
  const members = (c: Chat) => c.members?.map((id) => faceOf(chats.find((m) => m.id === id)!));
  return (
    <div className="absolute inset-0 bg-white">
      <div aria-hidden className="absolute inset-x-[16px] top-[62px] flex items-center justify-between">
        <div className={`flex h-[44px] w-[70px] items-center justify-center gap-[8px] rounded-full ${glass}`}>
          <span className="flex size-[26px] items-center justify-center rounded-full" style={{ background: palette.surface }}>
            <User size={15} weight="fill" color={palette.secondary} />
          </span>
          <CaretDown size={14} weight="bold" color={palette.text} />
        </div>
        <span className="flex items-center gap-[5px] text-[15px] font-medium" style={{ color: palette.secondary }}>
          1 connected <CaretDown size={12} weight="bold" />
        </span>
        <div className={`flex size-[44px] items-center justify-center rounded-full ${glass}`}>
          <Plus size={22} weight="bold" color={palette.text} />
        </div>
      </div>
      <ul className="absolute inset-x-0 top-[116px]">
        {sorted.map((c) => (
          <motion.li key={c.id} layout transition={conversation}>
            <button
              type="button"
              onClick={() => open(c.id)}
              className={`flex w-full items-center gap-[12px] px-[16px] py-[10px] text-left ${focus} active:bg-black/[0.04]`}
            >
              <AvatarWithStatus face={faceOf(c)} members={members(c)} unread={c.unread} size={46} />
              <span className="flex min-w-0 flex-1 flex-col gap-[4px]">
                <span className="flex items-baseline gap-[8px]">
                  <span className="flex-1 truncate text-[17px] font-semibold" style={{ color: palette.text }}>
                    {c.name}
                  </span>
                  <span className="text-[15px]" style={{ color: c.unread > 0 ? palette.text : palette.tertiary }}>
                    {clock(c.at)}
                  </span>
                </span>
                <span className="flex items-center gap-[6px] text-[15px]">
                  <span className="flex min-w-0 flex-1">{preview(c, chats)}</span>
                  {c.unread > 0 && (
                    <span
                      className="flex h-[18px] min-w-[18px] items-center justify-center rounded-full px-[6px] text-[11px] font-bold text-white"
                      style={{ background: palette.accentFill }}
                      aria-label={`${c.unread} unread`}
                    >
                      {c.unread}
                    </span>
                  )}
                </span>
              </span>
            </button>
          </motion.li>
        ))}
      </ul>
      <div aria-hidden className={`absolute bottom-[39px] left-1/2 flex -translate-x-1/2 gap-[4px] rounded-full p-[5px] ${glass}`}>
        {[
          { t: "Bots", I: Chats, on: true },
          { t: "State", I: CardsThree, on: false },
        ].map(({ t, I, on }) => (
          <span
            key={t}
            className="flex h-[50px] w-[76px] flex-col items-center justify-center gap-[3px] rounded-full text-[11px] font-medium"
            style={{ color: on ? palette.text : palette.secondary, background: on ? "rgba(20,20,20,0.08)" : undefined }}
          >
            <I size={22} weight="fill" />
            {t}
          </span>
        ))}
      </div>
      <HomeIndicator />
    </div>
  );
}

/* ----------------------------------------------------------------- Thread */

function PermissionCard({ p, respond }: { p: Permission; respond: (outcome: string) => void }) {
  const [expanded, setExpanded] = useState(false);
  const pending = p.outcome === null;
  const options = [
    { label: "Allow once", outcome: "Allowed once", strong: true, color: palette.text },
    { label: "Always allow", outcome: "Always allowed", strong: false, color: palette.text },
    { label: "Deny", outcome: "Denied", strong: false, color: palette.danger },
  ];
  return (
    <div className="mr-[40px] flex flex-col gap-[10px] rounded-[22px] p-[16px]" style={{ background: palette.bubbleAgent }}>
      <div className="flex items-center gap-[8px]">
        <span className="text-[17px] font-semibold" style={{ color: palette.text }}>
          Wants to change files
        </span>
        {pending && <span className="size-[7px] rounded-full" style={{ background: palette.warning }} />}
      </div>
      <span className="text-[15px]" style={{ fontFamily: mono, color: palette.secondary }}>
        {p.title}
      </span>
      <span className="flex items-center gap-[6px] text-[12px]" style={{ color: palette.tertiary }}>
        <Desktop size={14} weight="fill" /> Runs on MacBook-Pro-3 · trailmix
      </span>
      <button
        type="button"
        aria-expanded={expanded}
        onClick={() => setExpanded(!expanded)}
        className={`flex items-center justify-between rounded-[6px] text-[12px] font-medium ${focus}`}
        style={{ color: palette.secondary }}
      >
        Details
        <CaretRight size={13} weight="bold" className={`transition-transform duration-200 ${expanded ? "rotate-90" : ""}`} />
      </button>
      <AnimatePresence initial={false}>
        {expanded && (
          <motion.div
            initial={{ height: 0, opacity: 0 }}
            animate={{ height: "auto", opacity: 1 }}
            exit={{ height: 0, opacity: 0 }}
            transition={{ type: "spring", duration: 0.3, bounce: 0 }}
            className="overflow-hidden"
          >
            <div className="flex items-center gap-[6px] pb-[4px] text-[12px]" style={{ fontFamily: mono, color: palette.secondary }}>
              <span className="flex-1">pace.js</span>
              <span style={{ color: palette.added }}>+2</span>
              <span style={{ color: palette.removed }}>−2</span>
            </div>
            <pre className="overflow-x-auto rounded-[8px] p-[8px] text-[11px] leading-[1.45]" style={{ fontFamily: mono, background: palette.background }}>
              {paceDiff.map((l) => (
                <div key={l} style={{ color: l[0] === "+" ? palette.added : l[0] === "-" ? palette.removed : palette.secondary }}>
                  {l}
                </div>
              ))}
            </pre>
          </motion.div>
        )}
      </AnimatePresence>
      {pending ? (
        <div className="mt-[4px] overflow-hidden rounded-[14px]" style={{ background: palette.background }}>
          {options.map((o, i) => (
            <button
              key={o.label}
              type="button"
              disabled={p.answering !== null}
              onClick={() => respond(o.outcome)}
              className={`flex min-h-[46px] w-full items-center px-[14px] text-left text-[17px] ${focus} -outline-offset-2 active:bg-black/[0.04]`}
              style={{
                color: o.color,
                fontWeight: o.strong ? 600 : 400,
                borderTop: i ? `0.5px solid ${palette.border}` : undefined,
                opacity: p.answering === null || p.answering === o.outcome ? 1 : 0.4,
              }}
            >
              <span className="flex-1">{o.label}</span>
              {p.answering === o.outcome && (
                <span className="size-[14px] animate-spin rounded-full border-2 border-black/15 border-t-black/60 motion-reduce:animate-none" />
              )}
            </button>
          ))}
        </div>
      ) : (
        <span className="text-[12px] font-medium" style={{ color: palette.secondary }}>
          {p.outcome}
        </span>
      )}
    </div>
  );
}

/** WorkingIndicator: the live activity line with its timer. */
function Working({ activity, since }: { activity: NonNullable<Chat["activity"]>; since: number }) {
  const [s, setS] = useState(since);
  useEffect(() => {
    const id = setInterval(() => setS((v) => v + 1), 1000);
    return () => clearInterval(id);
  }, []);
  const ink = activity.needs ? palette.warning : palette.secondary;
  return (
    <div className="mr-[40px] flex w-fit items-center gap-[8px] rounded-[22px] px-[16px] py-[12px]" style={{ background: palette.bubbleAgent }} role="status">
      <Orb color={ink} />
      <span className="truncate text-[15px]" style={{ color: ink }}>
        {activity.text}
      </span>
      <span className="text-[13px] tabular-nums" style={{ color: palette.tertiary }}>
        {Math.floor(s / 60)}:{String(s % 60).padStart(2, "0")}
      </span>
    </div>
  );
}

function Row({ entry, prev, chats, respond }: { entry: Entry; prev?: Entry; chats: Chat[]; respond: (id: string, outcome: string) => void }) {
  const start = !prev || prev.kind !== entry.kind || (prev.kind === "agent" && entry.kind === "agent" && prev.author !== entry.author);
  switch (entry.kind) {
    case "separator":
      return (
        <p className="pt-[10px] text-center text-[13px]" style={{ color: palette.tertiary }}>
          {entry.text}
        </p>
      );
    case "user":
      return (
        <div className={`flex flex-col items-end gap-[4px] pl-[56px] ${start ? "pt-[12px]" : "pt-[4px]"}`}>
          <p className="rounded-[22px] px-[16px] py-[10px] text-[17px] leading-[22px] whitespace-pre-wrap" style={{ background: palette.bubbleUser, color: palette.text }}>
            {entry.text}
          </p>
        </div>
      );
    case "agent": {
      const author = entry.author ? chats.find((c) => c.id === entry.author) : undefined;
      return (
        <div className={`flex flex-col items-start gap-[4px] pr-[40px] ${start ? "pt-[12px]" : "pt-[4px]"}`}>
          {author && start && (
            <span className="flex items-center gap-[8px] text-[15px] font-medium" style={{ color: swatch(author.color) }}>
              <CharacterAvatar shape={author.shape} color={author.color} size={26} />
              {author.name}
            </span>
          )}
          <p className="rounded-[22px] px-[16px] py-[10px] text-[17px] leading-[22px]" style={{ background: palette.bubbleAgent, color: palette.text }}>
            <Inline text={entry.text} />
          </p>
        </div>
      );
    }
    case "permission":
      return (
        <div className="pt-[12px]">
          <PermissionCard p={entry.permission} respond={(o) => respond(entry.id, o)} />
        </div>
      );
  }
}

function Composer({ chat, send, stop }: { chat: Chat; send: (text: string) => void; stop: () => void }) {
  const [draft, setDraft] = useState("");
  const busy = chat.activity !== null;
  const group = !!chat.members;
  const placeholder = group ? `Message ${chat.name} · @ to ask one bot` : busy ? `Queue a message for ${chat.name}` : `Ask ${chat.name}`;
  const state = draft.trim() ? "send" : busy && !group ? "stop" : group ? "send" : "call";
  const enabled = state !== "send" || !!draft.trim();
  const submit = (e: React.FormEvent) => {
    e.preventDefault();
    if (state === "stop") return stop();
    if (!draft.trim()) return;
    send(draft.trim());
    setDraft("");
  };
  const inputRef = useRef<HTMLInputElement>(null);
  return (
    <form onSubmit={submit} className="absolute inset-x-[16px] bottom-[49px] z-20 flex items-end gap-[8px]">
      {!group && (
        <button
          type="button"
          aria-label="Add files"
          onClick={() => inputRef.current?.focus()}
          className={`flex size-[47px] shrink-0 items-center justify-center rounded-full ${glass} ${focus} ${press}`}
        >
          <Plus size={20} color={palette.text} />
        </button>
      )}
      <div className={`flex h-[54px] min-w-0 flex-1 items-center gap-[8px] rounded-[27px] pr-[6px] pl-[18px] ${glass} focus-within:shadow-[0_0_0_1px_rgba(0,0,0,0.25),0_6px_18px_rgba(0,0,0,0.07)]`}>
        <input
          ref={inputRef}
          value={draft}
          onChange={(e) => setDraft(e.target.value)}
          placeholder={placeholder}
          aria-label={placeholder}
          className="min-w-0 flex-1 bg-transparent text-[17px] outline-none placeholder:text-[#B4B4B4]"
          style={{ color: palette.text }}
        />
        <motion.button
          type="submit"
          layout
          transition={{ duration: 0.2, ease: [0.22, 1, 0.36, 1] }}
          disabled={!enabled}
          aria-label={state === "stop" ? "Stop" : state === "call" ? "Call" : "Send"}
          title={state === "call" ? "Voice call is in the app" : undefined}
          onClick={state === "call" ? (e) => { e.preventDefault(); inputRef.current?.focus(); } : undefined}
          className={`flex h-[34px] shrink-0 items-center justify-center rounded-full ${focus} ${press}`}
          style={{
            width: state === "call" ? 46 : 34,
            background: state === "send" && !enabled ? palette.accentDim : palette.accentFill,
            color: state === "send" && !enabled ? palette.tertiary : "#fff",
          }}
        >
          {state === "stop" ? <Stop size={14} weight="fill" /> : state === "call" ? <Waveform size={17} weight="bold" /> : <ArrowUp size={16} weight="bold" />}
        </motion.button>
      </div>
    </form>
  );
}

export function Thread({
  chat,
  chats,
  back,
  send,
  respond,
  stop,
}: {
  chat: Chat;
  chats: Chat[];
  back: () => void;
  send: (text: string) => void;
  respond: (entryId: string, outcome: string) => void;
  stop: () => void;
}) {
  const reduce = useReducedMotion();
  const scroller = useRef<HTMLDivElement>(null);
  const members = chat.members?.map((id) => faceOf(chats.find((m) => m.id === id)!));
  const count = chat.entries.length;
  useEffect(() => {
    const el = scroller.current;
    el?.scrollTo({ top: el.scrollHeight, behavior: reduce ? "auto" : "smooth" });
  }, [count, chat.activity, reduce]);
  // The approval was waiting 1:36 when the demo opens, like the screenshot.
  const since = chat.activity?.needs ? 96 : 0;
  return (
    <div className="absolute inset-0 bg-white">
      <div className="absolute inset-x-[16px] top-[62px] z-20 flex items-center justify-between">
        <button type="button" onClick={back} aria-label="Back" className={`flex size-[44px] items-center justify-center rounded-full ${glass} ${focus} ${press}`}>
          <CaretLeft size={22} weight="bold" color={palette.text} />
        </button>
        <div className="flex flex-col items-center">
          <span className="flex items-center gap-[7px]">
            {members ? <GroupAvatar members={members} size={22} /> : <CharacterAvatar {...faceOf(chat)} size={22} />}
            <span className="text-[16px] font-semibold" style={{ color: palette.text }}>
              {chat.name}
            </span>
          </span>
          <span className="flex items-center gap-[5px] text-[12px] font-medium" style={{ color: palette.secondary }}>
            <WifiHigh size={12} weight="bold" /> Connected
          </span>
        </div>
        <div aria-hidden className={`flex size-[44px] items-center justify-center rounded-full ${glass}`}>
          <Desktop size={24} weight="fill" color={palette.text} />
        </div>
      </div>
      <div aria-hidden className="pointer-events-none absolute inset-x-0 top-0 z-10 h-[132px] bg-[linear-gradient(#fff_78%,rgba(255,255,255,0))]" />
      <div
        ref={scroller}
        className="absolute inset-0 overflow-y-auto px-[16px] pt-[132px] pb-[112px] [scrollbar-width:none] [&::-webkit-scrollbar]:hidden"
        aria-label={`Conversation with ${chat.name}`}
        role="log"
      >
        <div className="flex min-h-full flex-col justify-end">
          <AnimatePresence initial={false}>
            {chat.entries.map((e, i) => (
              <motion.div key={e.id} layout="position" initial={{ opacity: 0, y: 12 }} animate={{ opacity: 1, y: 0 }} transition={conversation}>
                <Row entry={e} prev={chat.entries[i - 1]} chats={chats} respond={respond} />
              </motion.div>
            ))}
            {chat.activity && (
              <motion.div
                key={`activity-${chat.activity.text}`}
                layout="position"
                initial={{ opacity: 0, y: 12 }}
                animate={{ opacity: 1, y: 0 }}
                exit={{ opacity: 0 }}
                transition={conversation}
                className="pt-[12px]"
              >
                <Working activity={chat.activity} since={since} />
              </motion.div>
            )}
          </AnimatePresence>
        </div>
      </div>
      <div aria-hidden className="pointer-events-none absolute inset-x-0 bottom-0 z-10 h-[96px] bg-[linear-gradient(rgba(255,255,255,0),#fff_45%)]" />
      <Composer key={chat.id} chat={chat} send={send} stop={stop} />
      <HomeIndicator />
    </div>
  );
}
