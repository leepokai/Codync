import { HardDrives, LockKey, WifiHigh } from "@phosphor-icons/react/ssr";
import Eyebrow from "./eyebrow";
import Reveal from "./reveal";

const points = [
  {
    icon: HardDrives,
    title: "Your data stays home",
    body: "Bots, transcripts and memory live on your Mac, Windows PC or Linux machine. Agents run there with your own logins. No account needed, no cloud copy of your files or conversations.",
  },
  {
    icon: WifiHigh,
    title: "Tailscale, Wi-Fi or the internet",
    body: "On the same Wi-Fi or a Tailscale network the phone connects to your computer directly. Anywhere else it goes through the free relay, and you can run your own.",
  },
  {
    icon: LockKey,
    title: "End-to-end encrypted, even the pings",
    body: "Every message is sealed between your phone and your computer, so the relay only forwards ciphertext. Notification text is sealed to your phone's key too; the push relay never sees what your bots said.",
  },
];

export default function Privacy() {
  return (
    <section className="px-4 py-20 sm:px-6 md:py-28">
      <div className="mx-auto max-w-6xl">
        <Reveal>
          <Eyebrow>Privacy</Eyebrow>
          <h2 className="max-w-[36rem] text-3xl font-semibold tracking-tight text-neutral-50 md:text-4xl">
            Private by design.
          </h2>
          <p className="mt-5 max-w-[34rem] text-lg leading-relaxed text-neutral-400">
            The phone is a remote for a computer you own. Nothing about your work passes through anyone else in the clear.
          </p>
        </Reveal>

        <div className="mt-12 grid grid-cols-1 gap-4 md:grid-cols-3">
          {points.map(({ icon: Icon, title, body }, i) => (
            <Reveal key={title} delay={i * 0.05}>
              <div className="h-full rounded-3xl bg-neutral-900 p-8">
                <Icon size={26} className="text-neutral-50" />
                <h3 className="mt-5 text-xl font-semibold text-neutral-50">{title}</h3>
                <p className="mt-3 leading-relaxed text-neutral-400">{body}</p>
              </div>
            </Reveal>
          ))}
        </div>
      </div>
    </section>
  );
}
