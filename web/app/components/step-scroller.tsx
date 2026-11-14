"use client";

import { useEffect, useRef, useState } from "react";
import { useMotionValueEvent, useScroll } from "framer-motion";

type Step = { title: string; body: string; visual: React.ReactNode };

// Steps on the left, the picture for the one at the middle of the screen held on the right (desktop).
// On phones each step shows its own picture under its text.
export default function StepScroller({ steps }: { steps: Step[] }) {
  const refs = useRef<(HTMLLIElement | null)[]>([]);
  const [active, setActive] = useState(0);

  // The last step whose top has passed the middle of the screen.
  const update = () => {
    const middle = window.innerHeight / 2;
    setActive(Math.max(0, refs.current.findLastIndex((el) => el !== null && el.getBoundingClientRect().top < middle)));
  };
  const { scrollY } = useScroll();
  useMotionValueEvent(scrollY, "change", update);
  useEffect(update, []);

  return (
    <div className="grid grid-cols-1 md:grid-cols-[1fr_1fr] md:gap-16">
      <ol className="flex flex-col gap-20 md:gap-0 md:py-[20dvh]">
        {steps.map((step, i) => (
          <li
            key={step.title}
            ref={(el) => {
              refs.current[i] = el;
            }}
            className="md:flex md:min-h-[60dvh] md:flex-col md:justify-center"
          >
            <h3
              className={`text-3xl font-semibold tracking-tight text-neutral-50 transition-colors duration-500 ${i === active ? "" : "md:text-neutral-600"}`}
            >
              {step.title}
            </h3>
            <p
              className={`mt-4 max-w-[28rem] text-lg leading-relaxed text-neutral-400 transition-colors duration-500 ${i === active ? "" : "md:text-neutral-700"}`}
            >
              {step.body}
            </p>
            {/* Visually hidden from md up (the sticky copy shows instead), still read with its step. */}
            <div className="mx-auto mt-10 max-w-[17rem] md:sr-only">{step.visual}</div>
          </li>
        ))}
      </ol>
      <div aria-hidden className="hidden md:block">
        <div className="sticky top-16 flex h-[calc(100dvh-4rem)] items-center justify-center">
          {/* Every picture shares one grid cell and only the active one shows, so they cross-fade in place. */}
          <div className="grid w-full max-w-[min(19rem,calc((100dvh-7rem)*780/1696))]">
            {steps.map((step, i) => (
              <div
                key={step.title}
                className={`col-start-1 row-start-1 transition-opacity duration-500 motion-reduce:transition-none ${i === active ? "opacity-100" : "opacity-0"}`}
              >
                {step.visual}
              </div>
            ))}
          </div>
        </div>
      </div>
    </div>
  );
}
