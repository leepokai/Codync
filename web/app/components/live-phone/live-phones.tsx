"use client";

import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { AnimatePresence, motion, MotionConfig } from "framer-motion";
import Phone from "../phone";
import { approvalReply, initialChats, scriptedReply, type Chat, type Entry } from "./data";
import { ease } from "../motion";
import { Roster, SCREEN, StatusBar, Thread } from "./screens";

/** iOS push/pop: the new screen slides in over the old one, which drifts left. */
const push = { type: "spring", duration: 0.5, bounce: 0 } as const;

/** Lays out the 402 x 874 pt screen and scales it to the frame's width. */
function Screen({ children, label }: { children: React.ReactNode; label: string }) {
  const box = useRef<HTMLDivElement>(null);
  const [scale, setScale] = useState(0);
  useLayoutEffect(() => {
    const el = box.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setScale(el.clientWidth / SCREEN.w));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  return (
    <section ref={box} aria-label={label} className="absolute inset-0" style={{ fontFamily: '-apple-system, BlinkMacSystemFont, "SF Pro Text", "SF Pro", system-ui, sans-serif' }}>
      <div
        className="absolute top-0 left-0 overflow-hidden antialiased"
        style={{ width: SCREEN.w, height: SCREEN.h, transform: `scale(${scale})`, transformOrigin: "0 0", visibility: scale ? "visible" : "hidden" }}
      >
        {children}
      </div>
    </section>
  );
}

type Nav = { chat: string | null; dir: 1 | -1 };

export default function LivePhones() {
  const [chats, setChats] = useState<Chat[]>(initialChats);
  const [front, setFront] = useState<Nav>({ chat: "pacer", dir: 1 });
  const latest = useRef(chats);
  const shown = useRef<string | null>("pacer");
  useEffect(() => {
    latest.current = chats;
    shown.current = front.chat;
  });
  const timers = useRef<number[]>([]);
  const turns = useRef<Record<string, number>>({});
  const ids = useRef(0);
  useEffect(() => () => timers.current.forEach(clearTimeout), []);

  const later = (ms: number, fn: () => void) => timers.current.push(window.setTimeout(fn, ms));
  const update = (id: string, fn: (c: Chat) => Chat) => setChats((all) => all.map((c) => (c.id === id ? fn(c) : c)));
  const nextAt = (all: Chat[]) => Math.max(...all.map((c) => c.at)) + 1;
  const newId = () => `live-${ids.current++}`;
  const nextTurn = (id: string) => (turns.current[id] = (turns.current[id] ?? -1) + 1);

  /** The bot shows "Typing…", then its reply pops in; queued messages get a turn after. */
  const reply = (id: string, answer: (c: Chat) => { text: string; author?: string }) => {
    update(id, (c) => ({ ...c, activity: { text: "Typing…", needs: false } }));
    later(1600, () => {
      const chat = latest.current.find((c) => c.id === id);
      if (!chat) return;
      const r = answer(chat);
      const queued = chat.entries.filter((e) => e.kind === "user" && e.queued);
      setChats((all) =>
        all.map((c) =>
          c.id !== id
            ? c
            : {
                ...c,
                at: nextAt(all),
                unread: shown.current === id ? 0 : c.unread + 1,
                activity: null,
                entries: [
                  ...c.entries.map((e) => (e.kind === "user" && e.queued ? { ...e, queued: false } : e)),
                  { id: newId(), kind: "agent", text: r.text, author: r.author },
                ],
              },
        ),
      );
      const last = queued.at(-1);
      if (last && last.kind === "user") {
        const turn = nextTurn(id);
        later(500, () => reply(id, (c) => scriptedReply(c, last.text, turn)));
      }
    });
  };

  const send = (id: string, text: string) => {
    const busy = latest.current.find((c) => c.id === id)?.activity != null;
    setChats((all) =>
      all.map((c) => (c.id === id ? { ...c, at: nextAt(all), entries: [...c.entries, { id: newId(), kind: "user", text, queued: busy || undefined }] } : c)),
    );
    if (!busy) {
      const turn = nextTurn(id);
      later(500, () => reply(id, (c) => scriptedReply(c, text, turn)));
    }
  };

  const setPermission = (id: string, entryId: string, patch: Partial<Extract<Entry, { kind: "permission" }>["permission"]>) =>
    update(id, (c) => ({
      ...c,
      entries: c.entries.map((e) => (e.id === entryId && e.kind === "permission" ? { ...e, permission: { ...e.permission, ...patch } } : e)),
    }));

  const respond = (id: string, entryId: string, outcome: string) => {
    setPermission(id, entryId, { answering: outcome });
    later(450, () => {
      setPermission(id, entryId, { answering: null, outcome });
      reply(id, () => ({ text: approvalReply[outcome] }));
    });
  };

  const stop = (id: string) =>
    update(id, (c) => ({
      ...c,
      activity: null,
      entries: c.entries.map((e) =>
        e.kind === "permission" && e.permission.outcome === null ? { ...e, permission: { ...e.permission, outcome: "Cancelled" } } : e,
      ),
    }));

  const open = (setNav: (n: Nav) => void) => (id: string) => {
    update(id, (c) => ({ ...c, unread: 0 }));
    setNav({ chat: id, dir: 1 });
  };

  const screen = (nav: Nav, setNav: (n: Nav) => void) => {
    const chat = chats.find((c) => c.id === nav.chat);
    return (
      <AnimatePresence initial={false} custom={nav.dir}>
        <motion.div
          key={nav.chat ?? "roster"}
          custom={nav.dir}
          className="absolute inset-0 shadow-[-8px_0_24px_rgba(0,0,0,0.06)]"
          variants={{
            enter: (d: number) => ({ x: d > 0 ? "100%" : "-30%", zIndex: d > 0 ? 2 : 0 }),
            center: { x: 0, zIndex: 1 },
            exit: (d: number) => ({ x: d > 0 ? "-30%" : "100%", zIndex: d > 0 ? 0 : 2 }),
          }}
          initial="enter"
          animate="center"
          exit="exit"
          transition={push}
        >
          {chat ? (
            <Thread
              chat={chat}
              chats={chats}
              back={() => setNav({ chat: null, dir: -1 })}
              send={(t) => send(chat.id, t)}
              respond={(e, o) => respond(chat.id, e, o)}
              stop={() => stop(chat.id)}
            />
          ) : (
            <Roster chats={chats} open={open(setNav)} />
          )}
        </motion.div>
      </AnimatePresence>
    );
  };

  // Same markup on server and client; MotionConfig drops the movement under reduced motion.
  const enter = (delay: number, y: number) => ({ initial: { opacity: 0, y }, animate: { opacity: 1, y: 0 }, transition: { duration: 0.9, delay, ease } });

  return (
    <MotionConfig reducedMotion="user">
      <motion.div {...enter(0.1, 40)} className="relative w-[58%]">
        <Phone>
          <Screen label="Codync iPhone app demo: bot list">
            {/* Rows tapped here open their chat in the front phone. */}
            <Roster chats={chats} open={open(setFront)} />
            <StatusBar />
          </Screen>
        </Phone>
      </motion.div>
      <motion.div {...enter(0.25, 60)} className="absolute top-[10%] right-0 z-10 w-[58%]">
        <Phone>
          <Screen label="Codync iPhone app demo: chat">
            {screen(front, setFront)}
            <StatusBar />
          </Screen>
        </Phone>
      </motion.div>
    </MotionConfig>
  );
}
