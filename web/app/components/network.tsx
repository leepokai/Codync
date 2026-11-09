import { Desktop, Laptop, ShieldCheck, SquaresFour, Terminal } from "@phosphor-icons/react/ssr";
import Eyebrow from "./eyebrow";
import Points, { type Point } from "./points";
import Reveal from "./reveal";
import { CharacterAvatar } from "./live-phone/avatar";

// codync-network: bots on the user's other computers message each other peer to peer (docs/features/codync-network.md).
const exchange = [
  {
    icon: Laptop,
    computer: "MacBook Pro",
    bot: { name: "Lead", shape: "squircle", color: "blue" },
    line: "Messaged Reviewer on desk",
    text: "Can you review the payment PR before I merge it?",
  },
  {
    icon: Desktop,
    computer: "desk",
    bot: { name: "Reviewer", shape: "tablet", color: "violet" },
    line: "Message from Lead on MacBook Pro",
    text: "Two issues in src/pay.ts: a missing refund check and an unhandled timeout.",
  },
];

const points: Point[] = [
  {
    icon: ShieldCheck,
    title: "Each computer decides",
    body: "Rules on each computer say which bots may message its bots, and a bot can stay off the network. The sender can't get around them.",
  },
  {
    icon: SquaresFour,
    title: "Home and work apart",
    body: "A computer can be in several networks, each with its own members, invites and rules.",
  },
  {
    icon: Terminal,
    title: "Your own sessions too",
    body: "A Claude Code or Codex session on your computer can list your bots and message them through Codync's MCP server.",
  },
];

export default function Network() {
  return (
    <section id="network" className="scroll-mt-20 px-4 py-20 sm:px-6 md:py-28">
      <div className="mx-auto grid max-w-6xl grid-cols-1 items-center gap-12 md:grid-cols-[1fr_1.1fr] md:gap-20">
        <Reveal className="order-2 md:order-1">
          <div
            role="img"
            aria-label="Lead on the MacBook Pro asks Reviewer on the desk computer to review a pull request, and Reviewer answers with two issues"
            className="flex flex-col gap-5 rounded-3xl bg-neutral-900 p-5 md:p-6"
          >
            {exchange.map(({ icon: Icon, computer, bot, line, text }) => (
              <div key={computer}>
                <p className="flex items-center gap-2 px-2 text-sm">
                  <Icon size={16} weight="fill" className="text-neutral-50" />
                  <span className="font-medium text-neutral-50">{computer}</span>
                  <span className="text-neutral-500">· home</span>
                </p>
                <div className="mt-2 flex items-start gap-3 rounded-2xl bg-neutral-800 px-3 py-2.5">
                  <CharacterAvatar shape={bot.shape} color={bot.color} size={32} ink="#fafafa" />
                  <span className="flex min-w-0 flex-col">
                    <span className="text-sm font-medium text-neutral-50">{bot.name}</span>
                    <span className="text-xs text-neutral-500">{line}</span>
                    <span className="mt-1 text-sm leading-relaxed text-neutral-300">{text}</span>
                  </span>
                </div>
              </div>
            ))}
          </div>
        </Reveal>

        <Reveal className="order-1 md:order-2" delay={0.05}>
          <Eyebrow>Network (beta)</Eyebrow>
          <h2 className="text-3xl font-semibold tracking-tight text-neutral-50 md:text-4xl">
            Bots on your other computers, one message away.
          </h2>
          <p className="mt-5 max-w-[32rem] text-lg leading-relaxed text-neutral-400">
            Put your computers in a private network and a bot on your laptop can ask a bot on your desktop or server.
            The request runs in that bot&apos;s chat on its own computer, and the answer comes back. Computers connect
            peer to peer; when they can&apos;t, a relay passes only encrypted traffic. No Codync server in between.
          </p>
          <Points points={points} />
        </Reveal>
      </div>
    </section>
  );
}
