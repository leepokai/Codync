import Link from "next/link";
import Eyebrow from "./eyebrow";
import Halftone from "./halftone";
import Reveal from "./reveal";

// The price as the whole message: one number, said once and plainly.
export default function Cost() {
  return (
    <section id="cost" className="relative overflow-hidden px-4 py-24 sm:px-6 md:py-36">
      <Halftone corner="top-left" className="w-[30rem] max-w-[70vw] text-neutral-50 opacity-[0.12]" />
      <Halftone corner="bottom-right" className="w-[30rem] max-w-[70vw] text-neutral-50 opacity-[0.12]" />
      <Reveal className="relative mx-auto flex max-w-3xl flex-col items-center text-center">
        <Eyebrow>What it costs</Eyebrow>
        <p className="text-[8rem] leading-none font-semibold tracking-tighter text-neutral-50 md:text-[12rem]">$0</p>
        <p className="mt-8 max-w-[34rem] text-lg leading-relaxed text-neutral-400">
          Codync is 100% free. No hidden fees, no paid tier, no locked features. Bring your own subscription: your
          bots run on the Claude, ChatGPT or other agent plans you already pay for.
        </p>
        <Link href="/#compare" className="mt-8 font-medium text-neutral-200 underline underline-offset-4 hover:text-white">
          See how Codync compares with Grok Bot
        </Link>
      </Reveal>
    </section>
  );
}
