"use client";

import { useRef, useState, useEffect } from "react";
import { motion, useInView } from "framer-motion";

/* ─── Push Notification Demo ─── */
function PushNotificationDemo() {
  const ref = useRef<HTMLDivElement>(null);
  const inView = useInView(ref, { once: true, margin: "-60px" });
  const [showBanner, setShowBanner] = useState(false);

  useEffect(() => {
    if (inView) {
      const timer = setTimeout(() => setShowBanner(true), 500);
      return () => clearTimeout(timer);
    }
  }, [inView]);

  return (
    <div ref={ref} className="flex justify-center">
      {/* Faux iPhone */}
      <div className="relative w-[260px] aspect-[9/17] rounded-[2.2rem] bg-[#f0f0f0] overflow-hidden border-2 border-neutral-600 shadow-[0_10px_60px_rgba(0,0,0,0.6)]">
        {/* Notch */}
        <div className="absolute top-2 left-1/2 -translate-x-1/2 w-[90px] h-[26px] bg-black rounded-full z-20" />

        {/* Status bar */}
        <div className="absolute top-[10px] left-0 right-0 flex justify-between items-center px-6 z-10">
          <span className="text-[10px] font-semibold text-black/40">9:41</span>
          <div className="flex gap-1 text-[8px] text-black/40">
            <span>●●●●</span>
            <span>WiFi</span>
            <span>🔋</span>
          </div>
        </div>

        {/* Notification banner */}
        <motion.div
          initial={{ y: -120, opacity: 0 }}
          animate={showBanner ? { y: 0, opacity: 1 } : {}}
          transition={{ type: "spring", duration: 0.7, bounce: 0.25 }}
          className="absolute top-[42px] left-[10px] right-[10px] z-30"
        >
          <div className="flex items-start gap-2.5 p-2.5 bg-white rounded-2xl shadow-[0_4px_20px_rgba(0,0,0,0.08)]">
            <img
              src="/icon.png"
              alt="Codync"
              className="w-[30px] h-[30px] rounded-lg flex-shrink-0"
            />
            <div className="flex-1 min-w-0">
              <div className="flex justify-between items-center">
                <span className="text-[11px] font-semibold text-black">Session Complete</span>
                <span className="text-[9px] text-black/35">now</span>
              </div>
              <span className="text-[10px] text-black/50">my-project finished · $0.42</span>
            </div>
          </div>
        </motion.div>

        {/* Faux app grid */}
        <div className="absolute bottom-[14px] left-[14px] right-[14px] grid grid-cols-4 gap-2">
          {Array.from({ length: 8 }).map((_, i) => (
            <div key={i} className="aspect-square rounded-xl bg-black/[0.05]" />
          ))}
        </div>
      </div>
    </div>
  );
}

/* ─── Dynamic Island Demo ─── */
function DynamicIslandDemo() {
  const ref = useRef<HTMLDivElement>(null);
  const inView = useInView(ref, { once: true, margin: "-60px" });
  const [expanded, setExpanded] = useState(false);

  useEffect(() => {
    if (inView) {
      const timer = setTimeout(() => setExpanded(true), 600);
      return () => clearTimeout(timer);
    }
  }, [inView]);

  return (
    <div ref={ref} className="flex justify-center">
      <div className="relative w-[260px] pt-6">
        {/* Dynamic Island pill */}
        <motion.div
          className="mx-auto bg-black rounded-[22px] overflow-hidden"
          initial={{ width: 120, height: 36 }}
          animate={expanded ? { width: 240, height: 72 } : {}}
          transition={{ type: "spring", duration: 0.6, bounce: 0.2 }}
        >
          <motion.div
            className="flex items-center gap-3 px-4 h-full"
            initial={{ opacity: 0 }}
            animate={expanded ? { opacity: 1 } : {}}
            transition={{ delay: 0.3, duration: 0.3 }}
          >
            <div className="w-9 h-9 rounded-full bg-gradient-to-br from-white/20 to-white/5 flex items-center justify-center flex-shrink-0">
              <img src="/icon.png" alt="" className="w-6 h-6 rounded-md" />
            </div>
            <div className="flex-1 min-w-0">
              <div className="text-[11px] font-semibold text-white">my-project</div>
              <div className="text-[9px] text-white/50">Editing code...</div>
            </div>
            <div className="flex flex-col items-end gap-0.5">
              <span className="text-[10px] font-medium text-white/70">Opus</span>
              <span className="text-[9px] text-white/40">3:42</span>
            </div>
          </motion.div>
        </motion.div>

        {/* Lock Screen preview below */}
        <motion.div
          initial={{ opacity: 0, y: 10 }}
          animate={expanded ? { opacity: 1, y: 0 } : {}}
          transition={{ delay: 0.5, duration: 0.4 }}
          className="mt-4 mx-auto w-[240px] rounded-2xl bg-neutral-900/60 border border-neutral-800 p-3"
        >
          <div className="flex items-center gap-2 mb-2">
            <img src="/icon.png" alt="" className="w-5 h-5 rounded-md" />
            <span className="text-[10px] font-semibold text-white/80">Codync</span>
            <span className="text-[9px] text-white/30 ml-auto">2m ago</span>
          </div>
          <div className="space-y-1.5">
            {[
              { name: "my-project", model: "Opus", active: true },
              { name: "backend", model: "Sonnet", active: false },
            ].map((s) => (
              <div key={s.name} className="flex items-center gap-2">
                <div className={`w-1.5 h-1.5 rounded-full ${s.active ? "bg-white/80" : "bg-white/30"}`} />
                <span className="text-[10px] text-white/80">{s.name}</span>
                <span className="text-[8px] text-white/30 px-1.5 py-0.5 bg-white/5 rounded-full">{s.model}</span>
              </div>
            ))}
          </div>
        </motion.div>
      </div>
    </div>
  );
}

/* ─── Menu Bar Demo ─── */
function MenuBarDemo() {
  const ref = useRef<HTMLDivElement>(null);
  const inView = useInView(ref, { once: true, margin: "-60px" });

  return (
    <div ref={ref} className="flex justify-center">
      <div className="w-full max-w-[320px] relative">
        {/* Mac menu bar with notch */}
        <div className="relative">
          <div className="h-[26px] bg-gradient-to-r from-[#384f6b] to-[#4a6a8a] rounded-t-xl flex items-center px-3 relative overflow-visible">
            {/* Left: Apple + Finder */}
            <div className="flex items-center gap-3">
              <svg viewBox="0 0 24 24" fill="white" className="w-3 h-3 opacity-80">
                <path d="M18.71 19.5c-.83 1.24-1.71 2.45-3.05 2.47-1.34.03-1.77-.79-3.29-.79-1.53 0-2 .77-3.27.82-1.31.05-2.3-1.32-3.14-2.53C4.25 17 2.94 12.45 4.7 9.39c.87-1.52 2.43-2.48 4.12-2.51 1.28-.02 2.5.87 3.29.87.78 0 2.26-1.07 3.8-.91.65.03 2.47.26 3.64 1.98-.09.06-2.17 1.28-2.15 3.81.03 3.02 2.65 4.03 2.68 4.04-.03.07-.42 1.44-1.38 2.83" />
              </svg>
              <span className="text-[9px] text-white/70 font-medium">Finder</span>
            </div>

            {/* Center: Notch extending from above */}
            <div className="absolute left-1/2 -translate-x-1/2 -top-[10px] z-10">
              <div
                className="flex items-end justify-center px-3 pb-[5px]"
                style={{
                  width: 120,
                  height: 36,
                  background: "black",
                  borderRadius: "0 0 14px 14px",
                }}
              >
                <img src="/icon.png" alt="" className="w-[13px] h-[13px] rounded-sm opacity-90" />
              </div>
            </div>

            {/* Right: system icons */}
            <div className="ml-auto flex items-center gap-1.5 text-white/50 text-[9px]">
              <span>●●●</span>
            </div>
          </div>
        </div>

        {/* Popover — connected dark panel like real app */}
        <motion.div
          initial={{ opacity: 0, y: -5, scaleY: 0.95 }}
          animate={inView ? { opacity: 1, y: 0, scaleY: 1 } : {}}
          transition={{ delay: 0.4, duration: 0.3 }}
          className="origin-top mx-auto w-[280px] rounded-2xl bg-[#1a1a1a] border border-white/[0.08] shadow-[0_12px_50px_rgba(0,0,0,0.8)] overflow-hidden mt-1.5"
        >
          {/* Panel header — icon + controls */}
          <div className="flex items-center justify-between px-4 pt-3 pb-2">
            <img src="/icon.png" alt="" className="w-[18px] h-[18px] opacity-80" />
            <div className="flex items-center gap-2.5 text-white/30">
              <span className="text-[10px]">☀</span>
              <span className="text-[10px]">✦</span>
              <span className="text-[10px]">✕</span>
            </div>
          </div>

          {/* Session list */}
          <div className="px-3 pb-2 space-y-0.5">
            {[
              { name: "Codync", model: "Opus", task: "Editing code...", active: true },
              { name: "MyApp", model: "Sonnet", task: "Reading file", active: true },
              { name: "backend", model: "Haiku", task: null, active: false },
            ].map((s) => (
              <div key={s.name} className="flex items-center gap-2 px-2 py-2 rounded-lg hover:bg-white/5">
                <div className={`w-1.5 h-1.5 rounded-full flex-shrink-0 ${s.active ? "bg-white/80" : "bg-white/30"}`} />
                <span className="text-[11px] font-semibold text-white">{s.name}</span>
                <span className="text-[8px] text-white/40 px-1.5 py-0.5 bg-white/[0.08] rounded-full">{s.model}</span>
                <span className="text-[9px] text-white/30 ml-auto truncate max-w-[80px]">{s.task ?? "Idle"}</span>
              </div>
            ))}
          </div>

          {/* Footer */}
          <div className="border-t border-white/[0.06] px-4 py-2 flex justify-between items-center">
            <span className="text-[9px] text-white/25">3 sessions</span>
            <span className="text-[9px] text-white/25">$2.14 total</span>
          </div>
        </motion.div>
      </div>
    </div>
  );
}

/* ─── iCloud Sync Demo ─── */
function CloudKitSyncDemo() {
  const ref = useRef<HTMLDivElement>(null);
  const inView = useInView(ref, { once: true, margin: "-60px" });
  const [synced, setSynced] = useState(false);

  useEffect(() => {
    if (inView) {
      const timer = setTimeout(() => setSynced(true), 800);
      return () => clearTimeout(timer);
    }
  }, [inView]);

  return (
    <div ref={ref} className="flex justify-center">
      <div className="w-full max-w-[300px]">
        {/* Devices */}
        <div className="flex items-center justify-center gap-8">
          {/* Mac */}
          <motion.div
            initial={{ opacity: 0, x: -20 }}
            animate={inView ? { opacity: 1, x: 0 } : {}}
            transition={{ duration: 0.4 }}
            className="flex flex-col items-center gap-1.5"
          >
            <div className="w-16 h-11 rounded-lg bg-neutral-800 border border-neutral-700 flex items-center justify-center">
              <div className="w-10 h-6 rounded-sm bg-gradient-to-br from-[#384f6b] to-[#4a6a8a]" />
            </div>
            <div className="w-6 h-1 bg-neutral-700 rounded-full" />
            <span className="text-[9px] text-white/40">Mac</span>
          </motion.div>

          {/* Sync arrows */}
          <motion.div
            initial={{ opacity: 0 }}
            animate={synced ? { opacity: 1 } : {}}
            transition={{ duration: 0.3 }}
            className="flex flex-col items-center gap-1"
          >
            <svg viewBox="0 0 24 24" fill="none" className="w-5 h-5 text-white/40">
              <path d="M4 12h16M16 8l4 4-4 4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
            <motion.div
              animate={synced ? { scale: [1, 1.2, 1] } : {}}
              transition={{ duration: 0.5, delay: 0.2 }}
            >
              <svg viewBox="0 0 24 24" fill="none" className="w-6 h-6 text-white/60">
                <path d="M4 16.5C2.8 15.3 2 13.7 2 12c0-3.3 2.7-6 6-6 .3 0 .7 0 1 .1C10.1 4.2 12 3 14.2 3c3 0 5.5 2.3 5.8 5.2C22 9 23 10.8 23 13c0 2.8-2.2 5-5 5H6c-.7 0-1.4-.2-2-.5z" stroke="currentColor" strokeWidth="1.5" />
              </svg>
            </motion.div>
            <svg viewBox="0 0 24 24" fill="none" className="w-5 h-5 text-white/40">
              <path d="M20 12H4M8 16l-4-4 4-4" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </motion.div>

          {/* iPhone */}
          <motion.div
            initial={{ opacity: 0, x: 20 }}
            animate={inView ? { opacity: 1, x: 0 } : {}}
            transition={{ duration: 0.4, delay: 0.2 }}
            className="flex flex-col items-center gap-1.5"
          >
            <div className="w-8 h-14 rounded-lg bg-neutral-800 border border-neutral-700 flex items-center justify-center">
              <div className="w-5 h-9 rounded-sm bg-neutral-900" />
            </div>
            <span className="text-[9px] text-white/40">iPhone</span>
          </motion.div>
        </div>

        {/* Sync status */}
        <motion.div
          initial={{ opacity: 0 }}
          animate={synced ? { opacity: 1 } : {}}
          transition={{ delay: 0.5, duration: 0.3 }}
          className="mt-4 flex items-center justify-center gap-2"
        >
          <div className="w-2 h-2 rounded-full bg-green-500/60" />
          <span className="text-[11px] text-white/50">Synced via iCloud</span>
        </motion.div>
      </div>
    </div>
  );
}

/* ─── Primary Session Demo ─── */
function PrimarySessionDemo() {
  const [primary, setPrimary] = useState(0);

  const sessions = [
    { name: "Codync", model: "Opus" },
    { name: "MyApp", model: "Sonnet" },
    { name: "Backend", model: "Haiku" },
  ];

  return (
    <div className="flex justify-center">
      <div className="w-full max-w-[260px] rounded-2xl bg-neutral-900/50 border border-neutral-800 p-3 space-y-0.5">
        {sessions.map((s, i) => {
          const isPrimary = i === primary;
          return (
            <motion.div
              key={s.name}
              className={`flex items-center gap-2 px-2.5 py-2 rounded-xl cursor-pointer transition-colors ${isPrimary ? "bg-white/[0.06]" : "hover:bg-white/[0.03]"}`}
              onClick={() => setPrimary(i)}
              layout
            >
              <motion.div
                className={`w-1.5 h-1.5 rounded-full ${isPrimary ? "bg-white" : "bg-white/30"}`}
                layout
              />
              <span className={`text-[12px] ${isPrimary ? "font-semibold text-white" : "text-white/70"}`}>{s.name}</span>
              <span className="text-[9px] text-white/30 px-1.5 py-0.5 bg-white/5 rounded-full">{s.model}</span>
              <div className="ml-auto">
                <motion.div
                  className={`w-3 h-3 rounded-full border-2 flex items-center justify-center ${isPrimary ? "border-white" : "border-white/20"}`}
                >
                  {isPrimary && (
                    <motion.div
                      className="w-1.5 h-1.5 rounded-full bg-white"
                      initial={{ scale: 0 }}
                      animate={{ scale: 1 }}
                      transition={{ type: "spring", duration: 0.3 }}
                    />
                  )}
                </motion.div>
              </div>
            </motion.div>
          );
        })}
      </div>
    </div>
  );
}

/* ─── Zero Config Demo ─── */
function ZeroConfigDemo() {
  const ref = useRef<HTMLDivElement>(null);
  const inView = useInView(ref, { once: true, margin: "-60px" });
  const [step, setStep] = useState(0);

  useEffect(() => {
    if (inView) {
      const t1 = setTimeout(() => setStep(1), 400);
      const t2 = setTimeout(() => setStep(2), 1200);
      const t3 = setTimeout(() => setStep(3), 2000);
      return () => { clearTimeout(t1); clearTimeout(t2); clearTimeout(t3); };
    }
  }, [inView]);

  return (
    <div ref={ref} className="flex justify-center">
      <div className="w-full max-w-[300px] rounded-xl bg-[#1a1a1a] border border-neutral-800 overflow-hidden font-mono">
        {/* Window controls */}
        <div className="flex items-center gap-1.5 px-3 py-2 border-b border-neutral-800">
          <div className="w-2.5 h-2.5 rounded-full bg-red-500/60" />
          <div className="w-2.5 h-2.5 rounded-full bg-yellow-500/60" />
          <div className="w-2.5 h-2.5 rounded-full bg-green-500/60" />
          <span className="text-[9px] text-white/20 ml-2">Terminal</span>
        </div>
        <div className="p-3 space-y-1.5 text-[11px]">
          <div className="text-white/50">
            <span className="text-green-400/70">$ </span>
            <motion.span
              initial={{ opacity: 0 }}
              animate={step >= 1 ? { opacity: 1 } : {}}
              className="text-white/80"
            >
              brew install codync
            </motion.span>
          </div>
          {step >= 2 && (
            <motion.div
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              className="text-white/40"
            >
              =&gt; Installing Codync...
              <br />
              =&gt; Hooks configured automatically
            </motion.div>
          )}
          {step >= 3 && (
            <motion.div
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              className="text-green-400/70"
            >
              ✓ Ready. No login, no account.
            </motion.div>
          )}
        </div>
      </div>
    </div>
  );
}

/* ─── Demo Card Wrapper ─── */
function DemoCard({
  title,
  description,
  children,
  index,
  span = 1,
}: {
  title: string;
  description: string;
  children: React.ReactNode;
  index: number;
  span?: number;
}) {
  return (
    <motion.div
      initial={{ opacity: 0, y: 24 }}
      whileInView={{ opacity: 1, y: 0 }}
      viewport={{ once: true, margin: "-40px" }}
      transition={{ duration: 0.5, delay: index * 0.08, ease: [0.21, 0.47, 0.32, 0.98] }}
      className={`group relative rounded-2xl border border-neutral-800 bg-neutral-900/30 overflow-hidden ${span === 2 ? "sm:col-span-2" : ""}`}
    >
      {/* Visual demo area */}
      <div className="p-6 pb-4">
        {children}
      </div>
      {/* Text */}
      <div className="px-6 pb-6">
        <h3 className="font-semibold text-white text-sm mb-1.5">{title}</h3>
        <p className="text-sm text-neutral-400 leading-relaxed">{description}</p>
      </div>
    </motion.div>
  );
}

/* ─── Main Component ─── */
export default function VisualDemos() {
  return (
    <section className="px-6 py-20">
      <div className="max-w-4xl mx-auto">
        <motion.h2
          initial={{ opacity: 0 }}
          whileInView={{ opacity: 1 }}
          viewport={{ once: true }}
          className="text-2xl font-bold text-white text-center mb-4"
        >
          Everything you need to monitor Claude Code
        </motion.h2>

        {/* Cowork & Codex badges */}
        <motion.div
          initial={{ opacity: 0 }}
          whileInView={{ opacity: 1 }}
          viewport={{ once: true }}
          className="flex flex-wrap items-center justify-center gap-3 mb-12"
        >
          <span className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full border border-neutral-700 text-xs text-neutral-300 bg-neutral-900/50">
            <span className="w-1.5 h-1.5 rounded-full bg-green-500 animate-pulse" />
            Now supports Claude Code
          </span>
          <span className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full border border-neutral-800 text-xs text-neutral-500 bg-neutral-900/30">
            <span className="w-1.5 h-1.5 rounded-full bg-neutral-600" />
            Claude cowork — coming soon
          </span>
          <span className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded-full border border-neutral-800 text-xs text-neutral-500 bg-neutral-900/30">
            <span className="w-1.5 h-1.5 rounded-full bg-neutral-600" />
            Codex — coming soon
          </span>
        </motion.div>

        <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
          <DemoCard
            title="Push Notifications"
            description="Get notified when your Claude Code sessions complete, even in background. Never miss a status change."
            index={0}
          >
            <PushNotificationDemo />
          </DemoCard>

          <DemoCard
            title="Dynamic Island"
            description="Live session status on your Lock Screen and Dynamic Island. See what Claude is doing without opening the app."
            index={1}
          >
            <DynamicIslandDemo />
          </DemoCard>

          <DemoCard
            title="Menu Bar App"
            description="macOS menu bar app with instant session overview. Click to see all active Claude Code sessions."
            index={2}
          >
            <MenuBarDemo />
          </DemoCard>

          <DemoCard
            title="CloudKit Sync"
            description="Your session data syncs privately through iCloud. Zero server infrastructure needed."
            index={3}
          >
            <CloudKitSyncDemo />
          </DemoCard>

          <DemoCard
            title="Primary Session"
            description="Pin a session to show first on Dynamic Island. Only the primary session triggers completion alerts."
            index={4}
          >
            <PrimarySessionDemo />
          </DemoCard>

          <DemoCard
            title="Zero Configuration"
            description="One-click install, no login, no account. No analytics, no tracking. Your code never leaves your devices."
            index={5}
          >
            <ZeroConfigDemo />
          </DemoCard>
        </div>
      </div>
    </section>
  );
}
