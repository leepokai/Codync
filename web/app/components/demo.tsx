import Halftone from "./halftone";
import MacWindow from "./mac-window";
import Reveal from "./reveal";
import { FILM } from "../links";

// The real desktop app in a Mac window straight under the hero, then the launch film for those who'd rather watch.
export default function Demo() {
  return (
    <section id="product" className="relative scroll-mt-16 overflow-x-clip px-4 pb-24 sm:px-6 md:pb-32">
      <Halftone corner="bottom-left" className="w-[40rem] max-w-[80vw] text-neutral-50 opacity-[0.1]" />
      <Halftone corner="bottom-right" className="w-[40rem] max-w-[80vw] text-neutral-50 opacity-[0.1]" />
      <div className="relative mx-auto max-w-6xl">
        <Reveal>
          <MacWindow />
          <p className="mt-5 text-center text-sm text-neutral-500">
            The real Codync desktop app, running in your browser, with bots on a MacBook Pro and a Mac mini. Click a bot, open a room, send a message.
          </p>
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
