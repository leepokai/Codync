"use client";

import { motion } from "framer-motion";

export default function Hero() {
  return (
    <section className="relative flex flex-col items-center justify-center px-6 pt-32 pb-16 overflow-hidden">
      {/* Radial glow */}
      <div className="absolute inset-0 flex items-center justify-center pointer-events-none">
        <div className="w-[600px] h-[600px] rounded-full bg-white/[0.03] blur-[100px]" />
      </div>

      <motion.div
        initial={{ opacity: 0, y: 20 }}
        animate={{ opacity: 1, y: 0 }}
        transition={{ duration: 0.6 }}
        className="relative z-10 flex flex-col items-center text-center"
      >
        <motion.img
          src="/icon.png"
          alt="Codync"
          className="w-24 h-24 rounded-[22px] shadow-2xl shadow-white/5"
          initial={{ scale: 0.8, opacity: 0 }}
          animate={{ scale: 1, opacity: 1 }}
          transition={{ duration: 0.5, delay: 0.1 }}
        />

        <h1 className="mt-8 text-5xl sm:text-6xl font-bold tracking-tight text-white">
          Codync
        </h1>

        <p className="mt-4 text-lg sm:text-xl text-neutral-400 max-w-md leading-relaxed">
          Monitor your Claude Code sessions in real-time.
          <br />
          <span className="text-neutral-500">iPhone. Mac. Dynamic Island.</span>
        </p>

        <motion.a
          href="https://github.com/leepokai/CodePulse"
          target="_blank"
          rel="noopener noreferrer"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ delay: 0.35 }}
          className="mt-6 inline-flex items-center gap-2 px-4 py-2 rounded-full border border-neutral-800 text-sm text-neutral-400 hover:text-white hover:border-neutral-600 transition-colors"
        >
          <svg viewBox="0 0 24 24" fill="currentColor" className="w-4 h-4">
            <path d="M12 0C5.37 0 0 5.37 0 12c0 5.3 3.44 9.8 8.21 11.39.6.11.82-.26.82-.58v-2.03c-3.34.73-4.04-1.61-4.04-1.61-.55-1.39-1.34-1.76-1.34-1.76-1.09-.75.08-.73.08-.73 1.21.08 1.85 1.24 1.85 1.24 1.07 1.84 2.81 1.31 3.5 1 .11-.78.42-1.31.76-1.61-2.67-.3-5.47-1.33-5.47-5.93 0-1.31.47-2.38 1.24-3.22-.13-.3-.54-1.52.12-3.18 0 0 1.01-.32 3.3 1.23a11.5 11.5 0 016.02 0c2.3-1.55 3.3-1.23 3.3-1.23.66 1.66.25 2.88.12 3.18.77.84 1.24 1.91 1.24 3.22 0 4.61-2.81 5.63-5.48 5.92.43.37.81 1.1.81 2.22v3.29c0 .32.22.7.82.58C20.56 21.8 24 17.3 24 12c0-6.63-5.37-12-12-12z" />
          </svg>
          Open Source
        </motion.a>

        <motion.div
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ delay: 0.4 }}
          className="mt-4 flex items-center gap-3"
        >
          <span className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full border border-neutral-800 text-xs text-neutral-400">
            <span className="w-1.5 h-1.5 rounded-full bg-white animate-pulse" />
            Live Activity
          </span>
          <span className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full border border-neutral-800 text-xs text-neutral-400">
            <span className="w-1.5 h-1.5 rounded-full bg-neutral-400" />
            CloudKit Sync
          </span>
          <span className="inline-flex items-center gap-1.5 px-3 py-1 rounded-full border border-neutral-800 text-xs text-neutral-400">
            <span className="w-1.5 h-1.5 rounded-full bg-neutral-500" />
            APNs Push
          </span>
        </motion.div>
      </motion.div>
    </section>
  );
}
