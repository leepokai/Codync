"use client";

import { motion, MotionConfig } from "framer-motion";
import DownloadButton from "./download-button";
import GitHubStarButton from "./github-star-button";
import Halftone from "./halftone";
import LivePhones from "./live-phone/live-phones";
import { CharacterAvatar } from "./live-phone/avatar";
import { ease } from "./motion";

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
        <Halftone corner="top-right" className="w-[44rem] max-w-[90vw] text-neutral-50 opacity-[0.1]" />
        <div className="relative mx-auto grid max-w-6xl grid-cols-1 items-center gap-16 md:grid-cols-[1.2fr_1fr] md:gap-8">
          <div className="flex flex-col items-start text-left">
            <motion.p
              {...rise(0)}
              className="inline-flex items-center gap-2 rounded-full bg-neutral-900 py-1.5 pr-4 pl-1.5 text-sm text-neutral-300"
            >
              <span className="rounded-full bg-neutral-50 px-2.5 py-0.5 text-xs font-semibold text-neutral-950">100% free</span>
              Open source, no paid tier
            </motion.p>

            <motion.h1
              {...rise(0.05)}
              className="mt-8 text-[3.25rem] leading-[1.04] font-semibold tracking-tighter text-neutral-50 sm:text-7xl lg:text-[5.25rem]"
            >
              Bring your own{" "}
              {/* The sandbox, drawn as one: a dashed boundary with a bot sitting on its edge. */}
              <motion.span
                initial={{ opacity: 0, scale: 0.96 }}
                animate={{ opacity: 1, scale: 1 }}
                transition={{ duration: 0.8, delay: 0.35, ease }}
                className="relative mt-[0.12em] block w-fit rounded-[0.3em] border-2 border-dashed border-neutral-600 px-[0.24em] pb-[0.1em]"
              >
                sandbox.
                <span
                  aria-hidden
                  className="absolute -right-[0.26em] -bottom-[0.26em] flex size-[0.62em] items-center justify-center rounded-[0.18em] bg-neutral-50 [&_svg]:size-[80%]"
                >
                  <CharacterAvatar shape="squircle" color="blue" size={88} mood="working" />
                </span>
              </motion.span>
            </motion.h1>

            <motion.p {...rise(0.12)} className="mt-8 max-w-[33rem] text-lg leading-relaxed text-neutral-400">
              Codync turns Claude Code, Codex and 40+ agents into bots on a machine you own. Message them from anywhere.
            </motion.p>

            <motion.div {...rise(0.18)} className="mt-10 flex flex-wrap items-center gap-3">
              <DownloadButton />
              <GitHubStarButton />
            </motion.div>
          </div>

          {/* The live iPhones; the front one hangs below the back one, so reserve room for it. */}
          <div className="relative mx-auto mb-16 w-full max-w-[24rem] md:mb-14 md:max-w-[26rem]">
            <LivePhones />
          </div>
        </div>
      </section>
    </MotionConfig>
  );
}
