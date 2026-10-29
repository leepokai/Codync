import type { IconWeight } from "@phosphor-icons/react";
import type { ComponentType } from "react";

type PointIcon = ComponentType<{ size?: number; weight?: IconWeight; className?: string }>;

export type Point = { icon: PointIcon; title: string; body: string };

// Short points under a section's text: icon, title, one or two sentences. `className` sets the margin and columns.
export default function Points({
  points,
  className = "mt-10 sm:grid-cols-3 md:grid-cols-1",
  weight,
}: {
  points: Point[];
  className?: string;
  weight?: IconWeight;
}) {
  return (
    <ul className={`grid grid-cols-1 gap-6 ${className}`}>
      {points.map(({ icon: Icon, title, body }) => (
        <li key={title}>
          <Icon size={24} weight={weight} className="text-neutral-50" />
          <p className="mt-3 font-medium text-neutral-50">{title}</p>
          <p className="mt-1 text-sm leading-relaxed text-neutral-400">{body}</p>
        </li>
      ))}
    </ul>
  );
}
