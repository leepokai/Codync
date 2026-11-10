import { DeviceMobile, HardDrives, Laptop, LockKey, ShieldCheck, WifiHigh } from "@phosphor-icons/react/ssr";
import Reveal from "./reveal";
import SectionHeader from "./section-header";

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

// The path a message takes, top to bottom: the phone, the relay that only forwards, the computer.
const path = [
  { icon: DeviceMobile, name: "Your phone", detail: "Seals each message to your computer's key" },
  { icon: ShieldCheck, name: "Relay", detail: "Forwards ciphertext it can't open" },
  { icon: Laptop, name: "Your computer", detail: "Bots, chats and memory, in the clear only here" },
];

export default function Privacy() {
  return (
    <section className="px-4 py-24 sm:px-6 md:py-32">
      <div className="mx-auto max-w-6xl">
        <SectionHeader label="Privacy" title="Private by design.">
          The phone is a remote for a computer you own. Nothing about your work passes through anyone else in the clear.
        </SectionHeader>

        <div className="mt-16 grid grid-cols-1 items-center gap-12 md:grid-cols-[1fr_1fr] md:gap-20">
          <Reveal>
            <ul className="flex flex-col gap-8">
              {points.map(({ icon: Icon, title, body }) => (
                <li key={title} className="flex gap-4">
                  <Icon size={24} className="mt-0.5 shrink-0 text-neutral-50" />
                  <div>
                    <h3 className="text-lg font-semibold text-neutral-50">{title}</h3>
                    <p className="mt-1.5 leading-relaxed text-neutral-400">{body}</p>
                  </div>
                </li>
              ))}
            </ul>
          </Reveal>
          <Reveal delay={0.05}>
            <div
              role="img"
              aria-label="A message goes from your phone, through a relay that only forwards ciphertext, to your computer. On the same Wi-Fi or Tailscale it skips the relay."
              className="rounded-3xl bg-neutral-900 p-6 md:p-8"
            >
              <ol className="flex flex-col">
                {path.map(({ icon: Icon, name, detail }, i) => (
                  <li key={name} className="flex flex-col">
                    {i > 0 ? <span aria-hidden className="my-1 ml-6 h-10 border-l-2 border-dashed border-neutral-700" /> : null}
                    <span className="flex items-center gap-4 rounded-2xl bg-neutral-950 p-4">
                      <span className="flex size-12 shrink-0 items-center justify-center rounded-xl bg-neutral-800">
                        <Icon size={22} className="text-neutral-50" />
                      </span>
                      <span>
                        <span className="block font-medium text-neutral-50">{name}</span>
                        <span className="block text-sm text-neutral-400">{detail}</span>
                      </span>
                    </span>
                  </li>
                ))}
              </ol>
              <p className="mt-6 text-sm leading-relaxed text-neutral-500">
                On the same Wi-Fi or a Tailscale network, the phone skips the relay and reaches the computer directly.
              </p>
            </div>
          </Reveal>
        </div>
      </div>
    </section>
  );
}
