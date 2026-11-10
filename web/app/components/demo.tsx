import MacWindow from "./mac-window";
import Reveal from "./reveal";
import { FILM } from "../links";

// The real desktop app in a Mac window, then the launch film for those who'd rather watch.
export default function Demo() {
  return (
    <section id="product" className="scroll-mt-16 px-4 py-24 sm:px-6 md:py-32">
      <div className="mx-auto max-w-6xl">
        <Reveal className="mx-auto max-w-[40rem] text-center">
          <h2 className="text-4xl font-semibold tracking-tighter text-neutral-50 md:text-5xl">Try the real app, right here.</h2>
          <p className="mt-5 text-lg leading-relaxed text-neutral-400">
            The Codync desktop app, running in your browser. Click a bot, open a room, send a message.
          </p>
        </Reveal>
        <Reveal className="mt-12 md:mt-16" delay={0.05}>
          <MacWindow />
        </Reveal>
        <Reveal className="mt-20 grid grid-cols-1 items-center gap-8 md:mt-28 md:grid-cols-[1fr_1.6fr] md:gap-16">
          <div>
            <h3 className="text-2xl font-semibold tracking-tight text-neutral-50">Rather watch?</h3>
            <p className="mt-3 max-w-[22rem] leading-relaxed text-neutral-400">The launch film shows Codync at work in under two minutes.</p>
          </div>
          {/* youtube-nocookie sets nothing until the visitor presses play. */}
          <iframe
            src={`https://www.youtube-nocookie.com/embed/${FILM}?rel=0`}
            title="Codync launch film"
            loading="lazy"
            allow="accelerometer; autoplay; clipboard-write; encrypted-media; gyroscope; picture-in-picture; web-share"
            allowFullScreen
            className="aspect-video w-full rounded-3xl"
          />
        </Reveal>
      </div>
    </section>
  );
}
