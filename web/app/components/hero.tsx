"use client";

import { motion, MotionConfig } from "framer-motion";
import { ArrowRight, DeviceMobile, LinuxLogo, Terminal, WindowsLogo } from "@phosphor-icons/react";
import { agentName, agents } from "./agent-mark";
import AgentRotator from "./agent-rotator";
import { AppleLogo } from "./apple-logo";
import DownloadButton from "./download-button";
import Halftone from "./halftone";
import { CharacterAvatar } from "./live-phone/avatar";
import { AppStoreButton, PlayStoreButton } from "./store-buttons";
import GitHubStarButton from "./github-star-button";
import { ease } from "./motion";
import { CLOUD_SECTION, GITHUB, PRODUCT_HUNT } from "../links";

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
      <section className="relative overflow-x-clip px-4 pt-12 pb-16 sm:px-6 md:pt-16 md:pb-20">
        <Halftone corner="top-right" className="w-[44rem] max-w-[90vw] text-neutral-50 opacity-[0.07]" />
        <Halftone corner="top-left" className="w-[44rem] max-w-[90vw] text-neutral-50 opacity-[0.07]" />

        <div className="relative mx-auto flex max-w-4xl flex-col items-center text-center">
          {/* The newest feature, linked to its card further down. */}
          <motion.a
            {...rise(0)}
            href={CLOUD_SECTION}
            className="group inline-flex items-center gap-2 rounded-full bg-neutral-900 py-1.5 pr-3.5 pl-1.5 text-sm text-neutral-200 transition hover:bg-neutral-800"
          >
            <span className="rounded-full bg-neutral-50 px-2.5 py-0.5 text-xs font-semibold text-neutral-950">New</span>
            Run your bots on a free cloud computer
            <ArrowRight size={14} weight="bold" className="text-neutral-400 transition group-hover:translate-x-0.5" />
          </motion.a>

          <motion.p
            {...rise(0.02)}
            className="mt-3 inline-flex flex-wrap items-center justify-center gap-2 rounded-full bg-neutral-900 py-1.5 pr-4 pl-1.5 text-sm"
          >
            <span className="rounded-full bg-neutral-50 px-2.5 py-0.5 text-xs font-semibold whitespace-nowrap text-neutral-950">100% free</span>
            <span className="rounded-full bg-neutral-50 px-2.5 py-0.5 text-xs font-semibold whitespace-nowrap text-neutral-950">Private</span>
            <span className="text-neutral-300">End-to-end encrypted, no paid tier</span>
          </motion.p>

          <motion.h1
            {...rise(0.04)}
            className="mt-8 text-[2.75rem] leading-[1.06] font-semibold tracking-tighter text-neutral-50 sm:text-6xl lg:text-7xl"
          >
            <span className="sr-only">Bring your {list.format(agents.map((agent) => agentName[agent]))} into Codync</span>
            <span aria-hidden className="flex flex-col items-center">
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

          <motion.p {...rise(0.1)} className="mt-8 max-w-[38rem] text-lg leading-relaxed text-neutral-400">
            The open-source Grok Bot alternative, 100% free. Persistent AI teammates on your own computer, for code and
            everyday life. Bring any of 40+ agents and the subscription you already pay for.
          </motion.p>

          <motion.div {...rise(0.16)} className="mt-10 flex flex-wrap items-center justify-center gap-3">
            <DownloadButton />
            <GitHubStarButton />
          </motion.div>
          <motion.div {...rise(0.18)} className="mt-3 flex flex-wrap justify-center gap-3">
            <AppStoreButton />
            <PlayStoreButton />
          </motion.div>

          <motion.p {...rise(0.2)} className="mt-8 flex flex-wrap items-center justify-center gap-x-4 gap-y-2 text-sm text-neutral-500">
            <span className="inline-flex items-center gap-1.5"><AppleLogo size={15} weight="fill" /> macOS</span>
            <span className="inline-flex items-center gap-1.5"><WindowsLogo size={15} weight="fill" /> Windows</span>
            <span className="inline-flex items-center gap-1.5"><LinuxLogo size={15} weight="fill" /> Linux</span>
            <span className="inline-flex items-center gap-1.5"><DeviceMobile size={15} weight="fill" /> iPhone</span>
            <span className="inline-flex items-center gap-1.5"><Terminal size={15} weight="bold" /> Terminal</span>
            <a href={GITHUB} className="underline underline-offset-4 hover:text-neutral-300">Apache 2.0 licensed</a>
          </motion.p>

          <motion.a
            {...rise(0.22)}
            href={PRODUCT_HUNT}
            target="_blank"
            rel="noopener noreferrer"
            className="mt-8 flex items-center gap-3 transition hover:opacity-80"
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
          </motion.a>
        </div>
      </section>
    </MotionConfig>
  );
}
