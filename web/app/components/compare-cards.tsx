import Link from "next/link";
import { ArrowRight } from "@phosphor-icons/react/ssr";
import Halftone from "./halftone";
import { rivals } from "../compare/rivals";

// One card per comparison page; the halftone corner turns with each card so the row has rhythm.
const corners = ["bottom-right", "top-left", "bottom-left", "top-right"] as const;
// All the cards on one row on large screens.
const columns: Record<number, string> = { 3: "lg:grid-cols-3", 4: "lg:grid-cols-4", 5: "lg:grid-cols-5" };

export default function CompareCards({ except }: { except?: string }) {
  const shown = rivals.filter((r) => r.slug !== except);
  return (
    <ul className={`grid grid-cols-1 gap-4 sm:grid-cols-2 ${columns[shown.length] ?? "lg:grid-cols-4"}`}>
      {shown.map((r, i) => (
        <li key={r.slug}>
          <Link
            href={`/compare/${r.slug}`}
            className="group flex h-full flex-col overflow-hidden rounded-3xl bg-neutral-900 transition hover:bg-neutral-800 active:scale-[0.98]"
          >
            <div className="relative h-28 overflow-hidden bg-neutral-950">
              <Halftone corner={corners[i % corners.length]} className="w-64 text-neutral-50 opacity-30" />
              <p className="absolute right-5 bottom-4 left-5 text-xl leading-tight font-semibold tracking-tight text-neutral-50">
                vs {r.name}
              </p>
            </div>
            <div className="flex flex-1 items-start justify-between gap-4 p-5">
              <p className="text-sm leading-relaxed text-neutral-400">{r.card}</p>
              <ArrowRight size={18} className="mt-0.5 shrink-0 text-neutral-500 transition group-hover:translate-x-0.5 group-hover:text-neutral-200" />
            </div>
          </Link>
        </li>
      ))}
    </ul>
  );
}
