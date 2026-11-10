import { HardDrives, LockKey, WifiHigh } from "@phosphor-icons/react/ssr";
import Reveal from "./reveal";

const points = [
  {
    icon: HardDrives,
    title: "Your data stays home",
    body: "Bots, transcripts and memory live on your Mac, Windows PC or Linux machine, or on a cloud computer in your own provider account. Agents run there with your own logins. No Codync copy of your files or conversations.",
  },
  {
    icon: WifiHigh,
    title: "Tailscale, Wi-Fi or the internet",
    body: "On the same Wi-Fi or a Tailscale network the phone connects to your computer directly. Anywhere else, with a free account, it goes through the relay, and you can run your own.",
  },
  {
    icon: LockKey,
    title: "End-to-end encrypted, even the pings",
    body: "Every message is sealed between your phone and your computer, so the relay only forwards ciphertext. Notification text is sealed to your phone's key too; the push relay never sees what your bots said.",
  },
];

// Three promises as rows: the claim on the left, how it holds on the right.
export default function Privacy() {
  return (
    <section className="px-4 py-24 sm:px-6 md:py-32">
      <div className="mx-auto max-w-6xl">
        <Reveal>
          <h2 className="max-w-[36rem] text-4xl font-semibold tracking-tighter text-neutral-50 md:text-5xl">
            Private by design.
          </h2>
          <p className="mt-5 max-w-[34rem] text-lg leading-relaxed text-neutral-400">
            The phone is a remote for a computer you own. Nothing about your work passes through anyone else in the clear.
          </p>
        </Reveal>

        <Reveal>
          <ul className="mt-14">
            {points.map(({ icon: Icon, title, body }) => (
              <li key={title} className="grid grid-cols-1 gap-3 border-t border-neutral-800 py-8 md:grid-cols-[1fr_1.4fr] md:gap-16 md:py-10">
                <h3 className="flex items-center gap-4 text-2xl font-semibold tracking-tight text-neutral-50">
                  <Icon size={26} className="shrink-0" />
                  {title}
                </h3>
                <p className="max-w-[36rem] text-lg leading-relaxed text-neutral-400">{body}</p>
              </li>
            ))}
          </ul>
        </Reveal>
      </div>
    </section>
  );
}
