import Image from "next/image";
import Link from "next/link";
import { Coffee, DiscordLogo, GithubLogo, WechatLogo } from "@phosphor-icons/react/ssr";
import DownloadButton from "./download-button";
import { rivals } from "../compare/rivals";
import { CHINA_COMMUNITY, DISCORD, EMAIL, GITHUB, KO_FI } from "../links";

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
    <header className="sticky top-0 z-30 bg-neutral-950/80 backdrop-blur-md">
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
            href={CHINA_COMMUNITY}
            aria-label="WeChat and Lark community for users in China"
            title="微信 / Lark 中国用户交流群"
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

// The footer's columns, by what a visitor is looking for.
const footerColumns: { title: string; links: [label: string, href: string][] }[] = [
  {
    title: "Product",
    links: [
      ["Docs", "/docs"],
      ["Free AI agent app", "/free-ai-agent-app"],
      ["Claude Code on iPhone", "/claude-code-iphone"],
      ["Codex on iPhone", "/codex-iphone"],
      ["codync-mem", "/codync-mem"],
    ],
  },
  { title: "Compare", links: [...rivals.map((r): [string, string] => [`vs ${r.name}`, `/compare/${r.slug}`]), ["All comparisons", "/compare"]] },
  {
    title: "Community",
    links: [
      ["GitHub", GITHUB],
      ["Discord", DISCORD],
      ["WeChat / Lark (China)", CHINA_COMMUNITY],
      ["Ko-fi", KO_FI],
      ["Contact", `mailto:${EMAIL}`],
    ],
  },
  {
    title: "Legal",
    links: [
      ["Terms", "/terms"],
      ["Privacy", "/privacy"],
      ["Code signing policy", "/code-signing"],
    ],
  },
];

export function SiteFooter() {
  return (
    <footer className="px-4 pt-16 pb-12 sm:px-6">
      <div className="mx-auto grid max-w-6xl grid-cols-2 gap-x-6 gap-y-10 text-sm md:grid-cols-[1.6fr_repeat(4,1fr)]">
        <div className="col-span-2 md:col-span-1">
          <Link href="/" className="flex items-center gap-2.5 font-semibold text-neutral-50">
            <Image src="/icon.png" alt="" width={28} height={28} className="rounded-[7px]" />
            Codync
          </Link>
          <p className="mt-4 max-w-[16rem] leading-relaxed text-neutral-500">
            100% free and open source. No hidden fees.
            <br />© {new Date().getFullYear()} leepokai
          </p>
        </div>
        {footerColumns.map(({ title, links }) => (
          <div key={title}>
            <p className="font-medium text-neutral-200">{title}</p>
            <ul className="mt-4 flex flex-col gap-2.5">
              {links.map(([label, href]) => (
                <li key={label}>
                  <Link href={href} className="text-neutral-500 transition hover:text-neutral-300">
                    {label}
                  </Link>
                </li>
              ))}
            </ul>
          </div>
        ))}
      </div>
    </footer>
  );
}
