import type { Icon as PhosphorIcon } from "@phosphor-icons/react";
import { CharacterAvatar } from "./live-phone/avatar";

// The app's sidebar in miniature, for the panels that draw it: a computer's label line and a bot's row.

export function ComputerLabel({ icon: Icon, name, detail }: { icon: PhosphorIcon; name: string; detail?: string }) {
  return (
    <p className="flex items-center gap-2 px-2 text-sm">
      <Icon size={16} weight="fill" className="text-neutral-50" />
      <span className="font-medium text-neutral-50">{name}</span>
      {detail ? <span className="text-neutral-500">{detail}</span> : null}
    </p>
  );
}

type BotRowProps = {
  name: string;
  shape: string;
  color: string;
  line: string;
  /** More under the line, such as a message; the avatar then sits at the top. */
  children?: React.ReactNode;
};

export function BotRow({ name, shape, color, line, children }: BotRowProps) {
  return (
    <div className={`flex gap-3 rounded-2xl bg-neutral-900 px-3 py-2.5 ${children ? "items-start" : "items-center"}`}>
      <CharacterAvatar shape={shape} color={color} size={32} ink="#fafafa" />
      <span className="flex min-w-0 flex-col">
        <span className="text-sm font-medium text-neutral-50">{name}</span>
        <span className="truncate text-xs text-neutral-400">{line}</span>
        {children}
      </span>
    </div>
  );
}
