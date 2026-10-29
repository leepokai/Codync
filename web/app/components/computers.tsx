import { Cloud, DeviceMobile, HardDrives, Key, Laptop, ShieldCheck } from "@phosphor-icons/react/ssr";
import Eyebrow from "./eyebrow";
import Points, { type Point } from "./points";
import Reveal from "./reveal";
import { CharacterAvatar } from "./live-phone/avatar";

// The desktop sidebar with several computers selected: one section per computer
// (docs/guides/accounts-and-ssh.md#desktop-computers-approvals-and-ssh).
const computers = [
  {
    icon: Laptop,
    name: "MacBook Pro",
    status: "This Mac",
    bots: [
      { name: "Scout", shape: "cloud", color: "green", line: "Added weeklyTotals with a test" },
      { name: "Reviewer", shape: "tablet", color: "violet", line: "Two issues in src/pace.js" },
    ],
  },
  {
    icon: HardDrives,
    name: "build-box",
    status: "Over SSH",
    bots: [{ name: "Pacer", shape: "hex", color: "orange", line: "Running the test suite…" }],
  },
  {
    icon: Cloud,
    name: "gpu-vm",
    status: "Over SSH",
    bots: [{ name: "Relay", shape: "squircle", color: "blue", line: "Training run finished" }],
  },
];

const points: Point[] = [
  {
    icon: Key,
    title: "Your SSH, your keys",
    body: "Codync uses the system's OpenSSH with your keys or ssh-agent. It never asks for a password, and your keys stay on this computer.",
  },
  {
    icon: ShieldCheck,
    title: "Checked host keys",
    body: "The first connection shows the host key to confirm. A changed key, or a different computer at that address, blocks the connection.",
  },
  {
    icon: DeviceMobile,
    title: "On your phone too",
    body: "Pair your iPhone with any of them from the same screen. Each computer approves each device itself.",
  },
];

export default function Computers() {
  return (
    <section className="px-4 py-20 sm:px-6 md:py-28">
      <div className="mx-auto grid max-w-6xl grid-cols-1 items-center gap-12 md:grid-cols-[1fr_1.1fr] md:gap-20">
        <Reveal className="order-2 md:order-1">
          <div
            role="img"
            aria-label="The Codync sidebar with three computers: MacBook Pro with Scout and Reviewer, build-box over SSH with Pacer, gpu-vm over SSH with Relay"
            className="flex flex-col gap-5 rounded-3xl bg-neutral-900 p-5 md:p-6"
          >
            {computers.map(({ icon: Icon, name, status, bots }) => (
              <div key={name}>
                <p className="flex items-center gap-2 px-2 text-sm">
                  <Icon size={16} weight="fill" className="text-neutral-50" />
                  <span className="font-medium text-neutral-50">{name}</span>
                  <span className="text-neutral-500">· {status}</span>
                  <span className="ml-auto size-2 rounded-full bg-neutral-50" />
                </p>
                <ul className="mt-2 flex flex-col gap-1">
                  {bots.map((b) => (
                    <li key={b.name} className="flex items-center gap-3 rounded-2xl bg-neutral-800 px-3 py-2.5">
                      <CharacterAvatar shape={b.shape} color={b.color} size={32} ink="#fafafa" />
                      <span className="flex min-w-0 flex-col">
                        <span className="text-sm font-medium text-neutral-50">{b.name}</span>
                        <span className="truncate text-xs text-neutral-400">{b.line}</span>
                      </span>
                    </li>
                  ))}
                </ul>
              </div>
            ))}
          </div>
        </Reveal>

        <Reveal className="order-1 md:order-2" delay={0.05}>
          <Eyebrow>Computers</Eyebrow>
          <h2 className="text-3xl font-semibold tracking-tight text-neutral-50 md:text-4xl">
            Every machine you SSH into, in one sidebar.
          </h2>
          <p className="mt-5 max-w-[32rem] text-lg leading-relaxed text-neutral-400">
            Add a server, a cloud VM or another desktop by its SSH alias. Its bots sit beside your own, grouped by
            computer, and a new bot can live on any of them. It needs codync-host on that machine; the desktop app on Mac,
            Windows and Linux connects over SSH.
          </p>
          <Points points={points} />
        </Reveal>
      </div>
    </section>
  );
}
