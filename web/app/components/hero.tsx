"use client";

import { motion, MotionConfig } from "framer-motion";
import { DeviceMobile, LinuxLogo, Terminal, WindowsLogo } from "@phosphor-icons/react";
import { agentName, type Agent } from "./agent-mark";
import AgentRotator from "./agent-rotator";
import { AppleLogo } from "./apple-logo";
import DownloadButton from "./download-button";
import Halftone from "./halftone";
import MacWindow from "./mac-window";
import MediaLabel from "./media-label";
import LivePhones from "./live-phone/live-phones";
import { CharacterAvatar } from "./live-phone/avatar";
import AppStoreButton from "./app-store-button";
import GitHubStarButton from "./github-star-button";
import { ease } from "./motion";
import { GITHUB, PRODUCT_HUNT } from "../links";

// Agents the headline cycles through.
const agents: Agent[] = ["claude", "codex", "gemini", "cursor", "copilot", "opencode"];

const list = new Intl.ListFormat("en", { type: "disjunction" });

export default function Hero() {
  // Same markup on server and client; MotionConfig drops the movement for reduced motion.
  const rise = (delay: number) => ({
    initial: { opacity: 0, y: 20 },
    animate: { opacity: 1, y: 0 },
    transition: { duration: 0.7, delay, ease },
  });

  return (
    <MotionConfig reducedMotion="user">
      <section className="relative overflow-hidden px-4 pt-14 pb-20 sm:px-6 md:pt-20 md:pb-28">
        <Halftone corner="top-right" className="w-[44rem] max-w-[90vw] text-neutral-50 opacity-[0.12]" />
        <Halftone corner="top-left" className="w-[30rem] max-w-[70vw] text-neutral-50 opacity-[0.08]" />
        <motion.div {...rise(0)} className="relative mb-10 flex justify-center md:mb-14">
          <a
            href={PRODUCT_HUNT}
            target="_blank"
            rel="noopener noreferrer"
            className="flex items-center gap-3 transition hover:opacity-80"
          >
            {/* eslint-disable-next-line @next/next/no-img-element -- live badge served by Product Hunt */}
            <img
              src="https://api.producthunt.com/widgets/embed-image/v1/featured.svg?post_id=1267264&theme=dark"
              alt="Codync on Product Hunt"
              width={250}
              height={54}
              className="h-10 w-auto"
            />
            <span className="text-sm font-medium text-neutral-300">#16 Product of the Day</span>
          </a>
        </motion.div>
        <div className="relative mx-auto grid max-w-6xl grid-cols-1 items-center gap-14 md:grid-cols-[1.15fr_1fr] md:gap-8">
          <div className="flex flex-col items-start text-left">
            <motion.div {...rise(0)} className="flex flex-col items-start gap-4">
              <span className="inline-flex items-center gap-2 rounded-full bg-neutral-900 py-1.5 pr-4 pl-1.5 text-sm">
                <span className="rounded-full bg-neutral-50 px-2.5 py-0.5 text-xs font-semibold whitespace-nowrap text-neutral-950">100% free</span>
                <span className="rounded-full bg-neutral-50 px-2.5 py-0.5 text-xs font-semibold whitespace-nowrap text-neutral-950">Private</span>
                <span className="text-neutral-300">End-to-end encrypted, no paid tier</span>
              </span>
            </motion.div>

            <motion.h1
              {...rise(0.04)}
              className="mt-8 text-[2.75rem] leading-[1.06] font-semibold tracking-tighter text-neutral-50 sm:text-6xl md:text-5xl lg:text-6xl xl:text-7xl"
            >
              <span className="sr-only">Bring your {list.format(agents.map((agent) => agentName[agent]))} into Codync</span>
              <span aria-hidden className="flex flex-col items-start">
                <span>Bring your</span>
                <AgentRotator agents={agents} />
                <span className="inline-flex items-center gap-x-[0.22em]">
                  into
                  <span className="inline-flex size-[0.95em] items-center justify-center rounded-[0.24em] bg-neutral-50 [&_svg]:size-[78%]">
                    <CharacterAvatar shape="squircle" color="blue" size={88} mood="working" />
                  </span>
                  Codync
                </span>
              </span>
            </motion.h1>

            <motion.p {...rise(0.1)} className="mt-8 max-w-[36rem] text-lg leading-relaxed text-neutral-400">
              The open-source Grok Bot alternative, 100% free. Persistent AI teammates on your own computer, for code
              and everyday life. Bring any of 40+ agents and the subscription you already pay for.
            </motion.p>

            <motion.div {...rise(0.16)} className="mt-10 flex flex-wrap items-center gap-3">
              <DownloadButton />
              <AppStoreButton />
              <GitHubStarButton />
            </motion.div>

            <motion.p {...rise(0.2)} className="mt-6 flex flex-wrap items-center gap-x-4 gap-y-2 text-sm text-neutral-500">
              <span className="inline-flex items-center gap-1.5"><AppleLogo size={15} weight="fill" /> macOS</span>
              <span className="inline-flex items-center gap-1.5"><WindowsLogo size={15} weight="fill" /> Windows</span>
              <span className="inline-flex items-center gap-1.5"><LinuxLogo size={15} weight="fill" /> Linux</span>
              <span className="inline-flex items-center gap-1.5"><DeviceMobile size={15} weight="fill" /> iPhone</span>
              <span className="inline-flex items-center gap-1.5"><Terminal size={15} weight="bold" /> Terminal</span>
              <a href={GITHUB} className="underline underline-offset-4 hover:text-neutral-300">Apache 2.0 licensed</a>
            </motion.p>
          </div>

          {/* The live iPhones; the front one hangs below the back one, so reserve room for it. */}
          <div className="relative mx-auto mb-16 w-full max-w-[26rem] md:mb-20 md:max-w-none">
            <LivePhones />
          </div>
        </div>

        <motion.div
          id="product"
          initial={{ opacity: 0, y: 40 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.9, delay: 0.25, ease }}
          className="relative mx-auto mt-16 max-w-6xl scroll-mt-24 md:mt-20"
        >
          <MediaLabel label="Live demo">
            The real Codync desktop app, running in your browser. Click a bot, open a room, send a message.
          </MediaLabel>
          <MacWindow />
        </motion.div>
      </section>
    </MotionConfig>
  );
}
