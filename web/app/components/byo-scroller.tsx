"use client";

import { useEffect, useRef, useState } from "react";
import { MotionConfig, useMotionValueEvent, useScroll } from "framer-motion";
import BlurSwap from "./blur-swap";

type Step = { word: string; id?: string; content: React.ReactNode };

// The sticky "Bring your own …" (desktop) whose last word follows the step at the middle of the screen.
export default function ByoScroller({ steps }: { steps: Step[] }) {
  const refs = useRef<(HTMLDivElement | null)[]>([]);
  const [active, setActive] = useState(0);

  // The last step whose top has passed the middle of the screen, so a jump below the section starts on the last word.
  const update = () => {
    const middle = window.innerHeight / 2;
    setActive(Math.max(0, refs.current.findLastIndex((el) => el !== null && el.getBoundingClientRect().top < middle)));
  };
  const { scrollY } = useScroll();
  useMotionValueEvent(scrollY, "change", update);
  useEffect(update, []);

  return (
    <MotionConfig reducedMotion="user">
      <div className="mx-auto grid max-w-6xl grid-cols-1 md:grid-cols-[1fr_1fr] md:gap-20">
        <div aria-hidden className="hidden md:block">
          <div className="sticky top-0 flex h-[100dvh] items-center">
            <p className="text-6xl leading-[1.05] font-semibold tracking-tighter text-neutral-50 lg:text-7xl">
              Bring your own{" "}
              <BlurSwap id={steps[active].word}>{steps[active].word}</BlurSwap>
            </p>
          </div>
        </div>
        <div className="flex flex-col gap-24 py-12 md:gap-0 md:py-[15dvh]">
          {steps.map((step, i) => (
            <div
              key={step.word}
              ref={(el) => {
                refs.current[i] = el;
              }}
              id={step.id}
              className="scroll-mt-16 md:flex md:min-h-[80dvh] md:flex-col md:justify-center"
            >
              {step.content}
            </div>
          ))}
        </div>
      </div>
    </MotionConfig>
  );
}
