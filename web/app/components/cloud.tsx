import { Check, Cloud, LockKey, Moon } from "@phosphor-icons/react/ssr";
import Eyebrow from "./eyebrow";
import Points, { type Point } from "./points";
import Reveal from "./reveal";
import { CharacterAvatar } from "./live-phone/avatar";

// Codync Cloud: a cloud computer on the user's own provider account (docs/plans/codync-cloud.md), drawn like the app's
// cloud computer card and its provider picker.
const providers = [
  { name: "Boat", detail: "EU · boat_…9abc", picked: true },
  { name: "InstaBox", detail: "US · ibk_…ifua", picked: false },
];

const points: Point[] = [
  {
    icon: Cloud,
    title: "Boat or InstaBox",
    body: "Boat runs in the EU and InstaBox in the US. Up to three cloud computers per account, each with its own name.",
  },
  {
    icon: Moon,
    title: "Sleeps when idle",
    body: "It goes to sleep while your bots have nothing to do. A message, a routine or a webhook wakes it.",
  },
  {
    icon: LockKey,
    title: "Keys kept apart",
    body: "Your provider key is stored encrypted and shows only its last four. On the cloud computer, bots run without sudo, and Anthropic, OpenAI and Gemini API keys and voice keys go through a keeper they can use but not read.",
  },
];

export default function CodyncCloud() {
  return (
    <section id="cloud" className="scroll-mt-20 px-4 py-20 sm:px-6 md:py-28">
      <div className="mx-auto grid max-w-6xl grid-cols-1 items-center gap-12 md:grid-cols-[1.1fr_1fr] md:gap-20">
        <Reveal>
          <Eyebrow>Codync Cloud (beta)</Eyebrow>
          <h2 className="text-3xl font-semibold tracking-tight text-neutral-50 md:text-4xl">
            No computer on all day? Run your bots in the cloud.
          </h2>
          <p className="mt-5 max-w-[32rem] text-lg leading-relaxed text-neutral-400">
            Codync Cloud starts a Linux computer for your bots on your own Boat or InstaBox account. Paste the
            provider&apos;s key once and pick a name. Codync charges nothing for it; the provider bills your account under
            its own plan. It needs a free Codync account, and every app reaches it like any other computer.
          </p>
          <Points points={points} />
        </Reveal>

        <Reveal delay={0.05}>
          <div
            role="img"
            aria-label="A cloud computer named Research, asleep, with its bot Scout, running on Boat in the EU; InstaBox in the US is the other provider"
            className="flex flex-col gap-6 rounded-3xl bg-neutral-900 p-5 md:p-6"
          >
            <div>
              <p className="flex items-center gap-2 px-2 text-sm">
                <Cloud size={16} weight="fill" className="text-neutral-50" />
                <span className="font-medium text-neutral-50">Research</span>
                <span className="text-neutral-500">· Sleeping, wakes when you message</span>
              </p>
              <div className="mt-2 flex items-center gap-3 rounded-2xl bg-neutral-800 px-3 py-2.5">
                <CharacterAvatar shape="cloud" color="green" size={32} ink="#fafafa" />
                <span className="flex min-w-0 flex-col">
                  <span className="text-sm font-medium text-neutral-50">Scout</span>
                  <span className="truncate text-xs text-neutral-400">Weekly report is ready</span>
                </span>
              </div>
            </div>
            <div>
              <p className="px-2 text-sm text-neutral-500">Runs on</p>
              <ul className="mt-2 flex flex-col gap-1">
                {providers.map((p) => (
                  <li key={p.name} className="flex items-center gap-3 rounded-2xl bg-neutral-800 px-3 py-2.5">
                    <span className="flex size-8 items-center justify-center rounded-lg bg-neutral-700">
                      <Cloud size={16} className="text-neutral-50" />
                    </span>
                    <span className="flex min-w-0 flex-1 flex-col">
                      <span className="text-sm font-medium text-neutral-50">{p.name}</span>
                      <span className="truncate text-xs text-neutral-400">{p.detail}</span>
                    </span>
                    {p.picked ? <Check size={14} weight="bold" className="text-neutral-50" /> : null}
                  </li>
                ))}
              </ul>
            </div>
          </div>
        </Reveal>
      </div>
    </section>
  );
}
