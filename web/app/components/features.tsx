import Image from "next/image";
import { BellRinging } from "@phosphor-icons/react/ssr";
import Halftone from "./halftone";
import Phone from "./phone";
import Reveal from "./reveal";
import { surfaces } from "./surfaces";

const fade = "[mask-image:linear-gradient(to_bottom,black_60%,transparent)]";

// What working with the bots is like. Radius rule for the page: buttons and chips are pills,
// panels are rounded-3xl, rows inside them rounded-2xl.
export default function Features() {
  return (
    <section className="px-4 py-24 sm:px-6 md:py-32">
      <div className="mx-auto max-w-6xl">
        <Reveal>
          <h2 className="max-w-[36rem] text-4xl font-semibold tracking-tighter text-neutral-50 md:text-5xl">
            A team of bots on your own computer.
          </h2>
          <p className="mt-5 max-w-[34rem] text-lg leading-relaxed text-neutral-400">
            Each bot keeps its own chat, memory, folder and agent. Message one, or put a few in a room.
          </p>
        </Reveal>

        <div className="mt-14 grid grid-cols-1 gap-4 md:grid-cols-3">
          <Reveal className="md:col-span-2 md:row-span-2">
            <div className="relative flex h-full flex-col overflow-hidden rounded-3xl bg-neutral-900 md:flex-row">
              <Halftone corner="bottom-left" className="w-96 text-neutral-50 opacity-[0.14]" />
              <div className="relative p-8 md:w-1/2 md:p-10">
                <h3 className="text-2xl font-semibold tracking-tight text-neutral-50">Put bots in a room</h3>
                <p className="mt-3 leading-relaxed text-neutral-400">
                  In a group chat every member answers in its own session. They read each other, disagree and hand
                  work back. Mention one with @ to ask just that bot.
                </p>
              </div>
              <div className="relative h-80 md:h-auto md:w-1/2">
                <div className="absolute inset-x-8 top-0 md:inset-x-6 md:top-10">
                  <Phone src="/screens/group.webp" alt="Ship room group chat: Reviewer flags two issues and asks Scout, who adds a guard" />
                </div>
              </div>
            </div>
          </Reveal>

          <Reveal delay={0.05}>
            <div className="flex h-full flex-col overflow-hidden rounded-3xl bg-neutral-900">
              <div className="p-8 pb-6">
                <h3 className="text-2xl font-semibold tracking-tight text-neutral-50">Just the answer</h3>
                <p className="mt-3 leading-relaxed text-neutral-400">
                  One reply per turn. Tool calls and thoughts wait in the full conversation.
                </p>
              </div>
              {/* A close-up of the reply, which sits low on the screen. */}
              <div className="relative mx-8 mb-8 min-h-56 flex-1 overflow-hidden rounded-2xl bg-white">
                <Image
                  src="/screens/answer.webp"
                  alt="Scout's reply: it added weeklyTotals to src/pace.js with a test, and all tests pass"
                  fill
                  sizes="(min-width: 768px) 320px, 90vw"
                  className="object-cover object-[50%_76%]"
                />
              </div>
            </div>
          </Reveal>

          <Reveal delay={0.1}>
            <div className="relative h-full overflow-hidden rounded-3xl bg-neutral-50 p-8">
              <Halftone className="w-56 text-neutral-950 opacity-[0.12]" />
              <BellRinging size={28} className="relative text-neutral-950" />
              <h3 className="relative mt-6 text-2xl font-semibold tracking-tight text-neutral-950">Only the pings that matter</h3>
              <p className="relative mt-3 leading-relaxed text-neutral-600">
                A notification when a bot needs you or finishes. Edits and commands arrive as cards: allow once, always allow or deny.
              </p>
            </div>
          </Reveal>

          <Reveal className="md:col-span-3" delay={0.05}>
            <div className="grid grid-cols-1 items-end gap-8 overflow-hidden rounded-3xl bg-neutral-900 md:grid-cols-[1fr_1.5fr] md:gap-12">
              <div className="p-8 pb-0 md:p-10 md:self-center">
                <h3 className="text-2xl font-semibold tracking-tight text-neutral-50">Without opening the app</h3>
                <p className="mt-3 max-w-[24rem] leading-relaxed text-neutral-400">
                  Widgets, Live Activities and the Dynamic Island show what your bots are doing on iPhone.
                </p>
              </div>
              <ul className={`grid h-64 grid-cols-3 gap-4 px-8 pt-2 md:h-80 md:pt-10 md:pr-10 md:pl-0 ${fade}`}>
                {surfaces.map((s) => (
                  <li key={s.src}>
                    <Phone src={s.src} alt={s.alt} />
                  </li>
                ))}
              </ul>
            </div>
          </Reveal>
        </div>
      </div>
    </section>
  );
}
