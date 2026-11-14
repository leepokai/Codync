import { Check, Cloud } from "@phosphor-icons/react/ssr";
import { BotRow, ComputerLabel } from "./sidebar-rows";

// Codync Cloud: a cloud computer on the user's own provider account (docs/plans/codync-cloud.md), drawn like the app's
// cloud computer card and its provider picker.
const providers = [
  { name: "Boat", detail: "EU · boat_…9abc", picked: true },
  { name: "InstaBox", detail: "US · ibk_…ifua", picked: false },
];

export function CloudPanel() {
  return (
    <div
      role="img"
      aria-label="A cloud computer named Research, asleep, with its bot Relay, running on Boat in the EU; InstaBox in the US is the other provider"
      className="flex flex-col gap-6"
    >
      <div className="flex flex-col gap-2">
        <ComputerLabel icon={Cloud} name="Research" detail="Sleeping, wakes when you message" />
        <BotRow name="Relay" shape="squircle" color="blue" line="Weekly report is ready" />
      </div>
      <div>
        <p className="px-2 text-sm text-neutral-500">Runs on</p>
        <ul className="mt-2 flex flex-col gap-1">
          {providers.map((p) => (
            <li key={p.name} className="flex items-center gap-3 rounded-2xl bg-neutral-900 px-3 py-2.5">
              <span className="flex size-8 items-center justify-center rounded-lg bg-neutral-800">
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
  );
}
