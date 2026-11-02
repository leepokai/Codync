import Image from "next/image";
import Link from "next/link";
import { Coffee, DiscordLogo, GithubLogo, WechatLogo } from "@phosphor-icons/react/ssr";
import DownloadButton from "./download-button";
import { DISCORD, EMAIL, GITHUB, KO_FI, WECHAT } from "../links";

const nav: [href: string, label: string][] = [
  ["/#product", "Product"],
  ["/docs", "Docs"],
  ["/compare", "Compare"],
  ["/#cost", "Pricing"],
  ["/#faq", "FAQ"],
];

// The header and footer every page shares.
export function SiteHeader() {
  return (
    <header className="sticky top-0 z-30 bg-black/80 backdrop-blur-md">
      <nav className="mx-auto grid h-16 max-w-6xl grid-cols-[1fr_auto] items-center px-4 sm:px-6 lg:grid-cols-[1fr_auto_1fr]">
        <Link href="/" className="flex items-center gap-2.5 font-semibold text-neutral-50">
          <Image src="/icon.png" alt="" width={28} height={28} className="rounded-[7px]" />
          Codync
        </Link>
        <div className="hidden items-center gap-1 lg:flex">
          {nav.map(([href, label]) => (
            <Link key={href} href={href} className="rounded-full px-4 py-2 text-sm text-neutral-400 transition hover:text-neutral-100">
              {label}
            </Link>
          ))}
        </div>
        <div className="flex items-center justify-end gap-2">
          <a
            href={KO_FI}
            aria-label="Buy me a coffee"
            title="Buy me a coffee"
            // Hidden on the narrowest phones, where it would push the header past the screen; the footer keeps the link.
            className="hidden rounded-full p-2 text-neutral-400 transition hover:text-neutral-100 min-[360px]:block"
          >
            <Coffee size={20} />
          </a>
          <a
            href={DISCORD}
            aria-label="Codync on Discord"
            title="Discord"
            className="hidden rounded-full p-2 text-neutral-400 transition hover:text-neutral-100 sm:block"
          >
            <DiscordLogo size={20} />
          </a>
          <Link
            href={WECHAT}
            aria-label="WeChat community for users in China"
            title="微信中国用户交流群"
            className="hidden rounded-full p-2 text-neutral-400 transition hover:text-neutral-100 sm:block"
          >
            <WechatLogo size={20} />
          </Link>
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
  ["Docs", "/docs"],
  ["Free AI agent app", "/free-ai-agent-app"],
  ["Claude Code on iPhone", "/claude-code-iphone"],
  ["Codex on iPhone", "/codex-iphone"],
  ["Compare", "/compare"],
  ["codync-mem", "/codync-mem"],
  ["Terms", "/terms"],
  ["Privacy", "/privacy"],
  ["Discord", DISCORD],
  ["WeChat (China)", WECHAT],
  ["Ko-fi", KO_FI],
  ["Contact", `mailto:${EMAIL}`],
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
          <br />© {new Date().getFullYear()} Leeporic LLC
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
