import Phone from "./phone";
import Eyebrow from "./eyebrow";
import Reveal from "./reveal";

// The iPhone screens outside the app.
const surfaces = [
  {
    src: "/screens/widget.webp",
    alt: "The Widget gallery: a medium Home Screen widget with Claude session and weekly usage bars",
    title: "Widgets",
    body: "Your bots and Claude or Codex usage on the Home Screen and Lock Screen.",
  },
  {
    src: "/screens/activity.webp",
    alt: "The Live Activity preview: Reviewer is running the test suite, with a thinking orb",
    title: "Live Activities",
    body: "A working bot, its current step and a thinking orb on the Lock Screen. Tap to return to the chat.",
  },
  {
    src: "/screens/island.webp",
    alt: "The Dynamic Island preview: the bot's avatar and a thinking orb beside the camera",
    title: "Dynamic Island",
    body: "The same activity beside the camera while you're in another app. The orb turns into a check when it's done.",
  },
];

export default function Surfaces() {
  return (
    <section className="px-4 py-20 sm:px-6 md:py-28">
      <div className="mx-auto max-w-6xl">
        <Reveal>
          <Eyebrow>Everywhere</Eyebrow>
          <h2 className="max-w-[36rem] text-3xl font-semibold tracking-tight text-neutral-50 md:text-4xl">
            On every surface.
          </h2>
          <p className="mt-5 max-w-[32rem] text-lg leading-relaxed text-neutral-400">
            You don&apos;t have to keep the app open to know what your bots are doing.
          </p>
        </Reveal>

        <div className="mt-12 grid grid-cols-1 gap-12 md:grid-cols-3 md:gap-8">
          {surfaces.map((s, i) => (
            <Reveal key={s.title} delay={i * 0.05}>
              <Phone src={s.src} alt={s.alt} className="mx-auto w-full max-w-[16rem] md:max-w-none" />
              <h3 className="mt-8 text-xl font-semibold text-neutral-50">{s.title}</h3>
              <p className="mt-2 leading-relaxed text-neutral-400">{s.body}</p>
            </Reveal>
          ))}
        </div>
      </div>
    </section>
  );
}
