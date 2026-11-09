"use client";

import { CaretDown, DeviceMobile, LinuxLogo, Terminal, WindowsLogo } from "@phosphor-icons/react";
import { AppleLogo } from "./apple-logo";
import { useOs, type Os } from "./use-os";
import { APP_STORE, DMG, LINUX, LINUX_ARM, WINDOWS } from "../links";

type Icon = typeof DeviceMobile | typeof AppleLogo;
type Target = { label: string; detail: string; href: string; Icon: Icon; button?: string };

// The main button reads "Download for <label>" unless `button` says otherwise.
const targets: Record<Os | "terminal", Target> = {
  mac: { label: "macOS", detail: "Apple silicon and Intel", href: DMG, Icon: AppleLogo },
  windows: { label: "Windows", detail: "Installer for x64 PCs", href: WINDOWS, Icon: WindowsLogo },
  linux: { label: "Linux", detail: "AppImage for x86_64", href: LINUX, Icon: LinuxLogo, button: "Download for Linux (x86_64)" },
  "linux-arm64": { label: "Linux (Arm)", detail: "AppImage for arm64", href: LINUX_ARM, Icon: LinuxLogo, button: "Download for Linux (arm64)" },
  ios: { label: "iPhone", detail: "App Store", href: APP_STORE, Icon: DeviceMobile, button: "Get the iPhone app" },
  terminal: { label: "Terminal", detail: "Over SSH, any machine", href: "/#install", Icon: Terminal },
};

const order = ["mac", "windows", "linux", "linux-arm64", "ios", "terminal"] as const;

// "Download for <your OS>" with a split menu for the other platforms; a native <details>.
export default function DownloadButton({ size = "lg" }: { size?: "sm" | "lg" }) {
  const os = useOs();
  const main = targets[os];
  const h = size === "sm" ? "h-9 text-sm" : "h-12";
  return (
    <div className="relative flex">
      <a
        href={main.href}
        className={`${h} inline-flex items-center gap-2 rounded-l-full bg-neutral-50 pr-4 pl-5 font-medium whitespace-nowrap text-neutral-950 transition hover:bg-white active:scale-[0.98]`}
      >
        <main.Icon size={size === "sm" ? 15 : 18} weight="fill" />
        {size === "sm" ? "Download" : (main.button ?? `Download for ${main.label}`)}
      </a>
      <details className="group">
        <summary
          aria-label="Other platforms"
          className={`${h} flex cursor-pointer list-none items-center rounded-r-full border-l border-neutral-300 bg-neutral-50 px-3 text-neutral-950 transition hover:bg-white [&::-webkit-details-marker]:hidden`}
        >
          <CaretDown size={14} weight="bold" className="transition-transform group-open:rotate-180" />
        </summary>
        <ul className="absolute right-0 z-40 mt-2 w-64 rounded-2xl bg-neutral-900 p-2 shadow-[0_20px_40px_rgba(0,0,0,0.6)]">
          {order
            .filter((key) => key !== os)
            .map((key) => {
              const { label, detail, href, Icon } = targets[key];
              return (
                <li key={key}>
                  <a href={href} className="flex items-center gap-3 rounded-xl px-3 py-2.5 transition hover:bg-neutral-800">
                    <Icon size={18} weight="fill" className="text-neutral-200" />
                    <span>
                      <span className="block text-sm font-medium text-neutral-100">{label}</span>
                      <span className="block text-xs text-neutral-500">{detail}</span>
                    </span>
                  </a>
                </li>
              );
            })}
        </ul>
      </details>
    </div>
  );
}
