"use client";

import { motion } from "framer-motion";

const features = [
  {
    icon: (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.5} className="w-5 h-5">
        <rect x="5" y="2" width="14" height="20" rx="4" />
        <path d="M9 2h6" strokeLinecap="round" />
        <circle cx="12" cy="5" r="1.5" fill="currentColor" stroke="none" />
      </svg>
    ),
    title: "Dynamic Island",
    description:
      "Live session status on your Lock Screen and Dynamic Island. See what Claude is doing without opening the app.",
  },
  {
    icon: (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.5} className="w-5 h-5">
        <rect x="2" y="4" width="20" height="14" rx="2" />
        <path d="M8 22h8M12 18v4" strokeLinecap="round" />
        <circle cx="12" cy="7" r="1" fill="currentColor" stroke="none" />
      </svg>
    ),
    title: "Menu Bar App",
    description:
      "macOS menu bar app with instant session overview. Click to see all active Claude Code sessions.",
  },
  {
    icon: (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.5} className="w-5 h-5">
        <path d="M12 3v3M12 18v3M3 12h3M18 12h3" strokeLinecap="round" />
        <path d="M5.6 5.6l2.2 2.2M16.2 16.2l2.2 2.2M5.6 18.4l2.2-2.2M16.2 7.8l2.2-2.2" strokeLinecap="round" />
        <circle cx="12" cy="12" r="4" />
      </svg>
    ),
    title: "Always-on Push",
    description:
      "Live Activity stays updated even when the app is closed. Never miss a status change.",
  },
  {
    icon: (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.5} className="w-5 h-5">
        <path d="M4 16.5C2.8 15.3 2 13.7 2 12c0-3.3 2.7-6 6-6 .3 0 .7 0 1 .1C10.1 4.2 12 3 14.2 3c3 0 5.5 2.3 5.8 5.2C22 9 23 10.8 23 13c0 2.8-2.2 5-5 5H6c-.7 0-1.4-.2-2-.5z" />
      </svg>
    ),
    title: "CloudKit Sync",
    description:
      "Your session data syncs privately through iCloud. Zero server infrastructure needed.",
  },
  {
    icon: (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.5} className="w-5 h-5">
        <rect x="6" y="3" width="12" height="18" rx="6" />
        <circle cx="12" cy="8" r="2" />
        <path d="M15 18H9" strokeLinecap="round" />
      </svg>
    ),
    title: "Apple Watch",
    description:
      "Session status on your wrist via Smart Stack widget. Glanceable progress at all times.",
  },
  {
    icon: (
      <svg viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth={1.5} className="w-5 h-5">
        <rect x="5" y="4" width="14" height="16" rx="2" />
        <path d="M9 4V2M15 4V2" strokeLinecap="round" />
        <circle cx="12" cy="12" r="2.5" />
        <path d="M12 9.5V7M12 17v-2.5" strokeLinecap="round" />
      </svg>
    ),
    title: "Privacy First",
    description:
      "No accounts, no analytics, no tracking. Code never leaves your devices. Everything stays in iCloud.",
  },
];

export default function Features() {
  return (
    <section className="px-6 py-20">
      <div className="max-w-4xl mx-auto">
        <motion.h2
          initial={{ opacity: 0 }}
          whileInView={{ opacity: 1 }}
          viewport={{ once: true }}
          className="text-2xl font-bold text-white text-center mb-12"
        >
          Everything you need to monitor Claude Code
        </motion.h2>

        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4">
          {features.map((feature, i) => (
            <motion.div
              key={feature.title}
              initial={{ opacity: 0, y: 20 }}
              whileInView={{ opacity: 1, y: 0 }}
              viewport={{ once: true, margin: "-30px" }}
              transition={{ duration: 0.4, delay: i * 0.08 }}
              className="group p-5 rounded-2xl border border-neutral-800 bg-neutral-900/30 hover:bg-neutral-900/60 hover:border-neutral-700 transition-all duration-300"
            >
              <div className="w-9 h-9 rounded-lg bg-neutral-800/80 border border-neutral-700/50 flex items-center justify-center text-neutral-300 mb-3 group-hover:text-white group-hover:border-neutral-600 transition-colors">
                {feature.icon}
              </div>
              <h3 className="font-semibold text-white text-sm mb-1.5">
                {feature.title}
              </h3>
              <p className="text-sm text-neutral-400 leading-relaxed">
                {feature.description}
              </p>
            </motion.div>
          ))}
        </div>
      </div>
    </section>
  );
}
