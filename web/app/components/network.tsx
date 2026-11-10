import { Desktop, Laptop } from "@phosphor-icons/react/ssr";
import { BotRow, ComputerLabel } from "./sidebar-rows";

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

export function NetworkPanel() {
  return (
    <div
      role="img"
      aria-label="Lead on the MacBook Pro asks Reviewer on the desk computer to review a pull request, and Reviewer answers with two issues"
      className="flex flex-col gap-5"
    >
      {exchange.map(({ icon, computer, bot, line, text }) => (
        <div key={computer} className="flex flex-col gap-2">
          <ComputerLabel icon={icon} name={computer} />
          <BotRow {...bot} line={line}>
            <span className="mt-1 text-sm leading-relaxed text-neutral-300">{text}</span>
          </BotRow>
        </div>
      ))}
    </div>
  );
}
