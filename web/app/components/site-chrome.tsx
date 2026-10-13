import Image from "next/image";
import Link from "next/link";
import { DiscordLogo, GithubLogo } from "@phosphor-icons/react/ssr";
import DownloadButton from "./download-button";
import { DISCORD, GITHUB } from "../links";

const nav: [href: string, label: string][] = [
  ["/#product", "Product"],
  ["/compare", "Compare"],
  ["/#cost", "Pricing"],
  ["/#faq", "FAQ"],
];

// The header and footer every page shares.
export function SiteHeader() {
  return (
    <header className="sticky top-0 z-30 bg-black/80 backdrop-blur-md">
      <nav className="mx-auto grid h-16 max-w-6xl grid-cols-[1fr_auto] items-center px-4 sm:px-6 md:grid-cols-[1fr_auto_1fr]">
        <Link href="/" className="flex items-center gap-2.5 font-semibold text-neutral-50">
          <Image src="/icon.png" alt="" width={28} height={28} className="rounded-[7px]" />
          Codync
        </Link>
        <div className="hidden items-center gap-1 md:flex">
          {nav.map(([href, label]) => (
            <Link key={href} href={href} className="rounded-full px-4 py-2 text-sm text-neutral-400 transition hover:text-neutral-100">
              {label}
            </Link>
          ))}
        </div>
        <div className="flex items-center justify-end gap-2">
          <a
            href={DISCORD}
            aria-label="Codync on Discord"
            title="Discord"
            className="hidden rounded-full p-2 text-neutral-400 transition hover:text-neutral-100 sm:block"
          >
            <DiscordLogo size={20} />
          </a>
          <a
            href={GITHUB}
            aria-label="Codync on GitHub"
            title="GitHub"
            className="rounded-full p-2 text-neutral-400 transition hover:text-neutral-100"
          >
            <GithubLogo size={20} />
          </a>
          <DownloadButton size="sm" />
        </div>
      </nav>
    </header>
  );
}

const footerLinks: [label: string, href: string][] = [
  ["Free AI agent app", "/free-ai-agent-app"],
  ["Claude Code on iPhone", "/claude-code-iphone"],
  ["Codex on iPhone", "/codex-iphone"],
  ["Compare", "/compare"],
  ["Terms", "/terms"],
  ["Privacy", "/privacy"],
  ["Discord", DISCORD],
  ["Contact", `${GITHUB}/issues`],
];

export function SiteFooter() {
  return (
    <footer className="px-4 py-12 sm:px-6">
      <div className="mx-auto flex max-w-6xl flex-col gap-6 text-sm text-neutral-500 sm:flex-row sm:items-center sm:justify-between">
        <p>
          100% free and open source. No hidden fees.{" "}
          <a href={GITHUB} className="text-neutral-300 underline underline-offset-4 hover:text-white">
            Code on GitHub
          </a>
        </p>
        <div className="flex flex-wrap gap-x-6 gap-y-2">
          {footerLinks.map(([label, href]) => (
            <Link key={label} href={href} className="transition hover:text-neutral-300">
              {label}
            </Link>
          ))}
        </div>
      </div>
    </footer>
  );
}
