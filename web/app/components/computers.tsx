import { CloudPanel } from "./cloud";
import { NetworkPanel } from "./network";
import Reveal from "./reveal";

const cards = [
  {
    id: "cloud",
    title: "Codync Cloud",
    body: "No computer on all day? Start a Linux sandbox on your own Boat or InstaBox account. It sleeps while your bots are idle and wakes when you message one. Codync charges nothing for it.",
    panel: <CloudPanel />,
  },
  {
    id: "network",
    title: "Network",
    body: "A bot on your laptop can ask a bot on your desktop or server, which works on its own computer and answers. Computers connect peer to peer, and each decides who may message its bots.",
    panel: <NetworkPanel />,
  },
];

// Past one computer: a sandbox in the cloud, and bots on different computers working together.
export default function Computers() {
  return (
    <section className="px-4 py-24 sm:px-6 md:py-32">
      <div className="mx-auto max-w-6xl">
        <Reveal>
          <h2 className="max-w-[36rem] text-4xl font-semibold tracking-tighter text-neutral-50 md:text-5xl">
            More than one sandbox.
          </h2>
          <p className="mt-5 max-w-[34rem] text-lg leading-relaxed text-neutral-400">
            Add servers over SSH from the desktop app, start one in the cloud, and let bots on different computers work
            together.
          </p>
        </Reveal>

        <div className="mt-14 grid grid-cols-1 gap-4 md:grid-cols-2">
          {cards.map((card, i) => (
            <Reveal key={card.id} delay={i * 0.05}>
              <article id={card.id} className="flex h-full scroll-mt-20 flex-col rounded-3xl bg-neutral-900 p-5 md:p-6">
                <div className="flex-1 rounded-2xl bg-neutral-950 p-4 md:p-5">{card.panel}</div>
                <div className="p-3 pt-8 md:p-4 md:pt-10">
                  <h3 className="flex items-center gap-3 text-2xl font-semibold tracking-tight text-neutral-50">
                    {card.title}
                    <span className="rounded-full bg-neutral-800 px-2.5 py-0.5 text-xs font-medium tracking-normal text-neutral-300">Beta</span>
                  </h3>
                  <p className="mt-3 leading-relaxed text-neutral-400">{card.body}</p>
                </div>
              </article>
            </Reveal>
          ))}
        </div>
      </div>
    </section>
  );
}
