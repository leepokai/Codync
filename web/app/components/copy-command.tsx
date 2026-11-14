"use client";

import { useState } from "react";
import { Check, Copy } from "@phosphor-icons/react";
import { track, type Platform } from "../analytics";

// A command to copy: with `platform`, an install command whose copy counts as a download.
export default function CopyCommand({ command, platform }: { command: string; platform?: Platform }) {
  const [copied, setCopied] = useState(false);

  async function copy() {
    if (platform) track("download_clicked", { platform });
    try {
      await navigator.clipboard.writeText(command);
      setCopied(true);
      setTimeout(() => setCopied(false), 1500);
    } catch {
      // Clipboard blocked (insecure context): the command stays selectable.
    }
  }

  return (
    <div className="flex items-center gap-3 rounded-xl bg-black py-3 pr-3 pl-4">
      <code className="min-w-0 flex-1 overflow-x-auto font-mono text-sm whitespace-nowrap text-neutral-200">
        {command}
      </code>
      <button
        type="button"
        onClick={copy}
        aria-label={copied ? "Copied" : "Copy command"}
        title={copied ? "Copied" : "Copy"}
        className="shrink-0 rounded-full p-2 text-neutral-400 transition hover:bg-neutral-800 hover:text-neutral-100 active:scale-95"
      >
        {copied ? <Check size={16} className="text-neutral-50" /> : <Copy size={16} />}
      </button>
    </div>
  );
}
