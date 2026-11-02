import Image from "next/image";
import { DeviceMobile, LinuxLogo, WindowsLogo } from "@phosphor-icons/react/ssr";
import { AppleLogo } from "./apple-logo";
import CopyCommand from "./copy-command";
import Eyebrow from "./eyebrow";
import Halftone from "./halftone";
import Reveal from "./reveal";
import { APP_STORE, DMG, INSTALL_SH, LINUX, LINUX_ARM, WINDOWS } from "../links";

export default function Install() {
  return (
    <section id="install" className="relative overflow-hidden px-4 py-20 sm:px-6 md:py-28">
      <Halftone className="w-[36rem] max-w-[80vw] text-neutral-50 opacity-[0.12]" />
      <div className="relative mx-auto max-w-6xl">
        <Reveal>
          <Eyebrow>Install</Eyebrow>
          <h2 className="text-3xl font-semibold tracking-tight text-neutral-50 md:text-4xl">Install once.</h2>
          <p className="mt-4 max-w-[36rem] text-lg leading-relaxed text-neutral-400">
            Put Codync on the computer your agents run on, then pair your phone from it.
          </p>
        </Reveal>

        <div className="mt-12 grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-4">
          <Reveal className="h-full">
            <div className="flex h-full flex-col rounded-3xl bg-neutral-900 p-8">
              <AppleLogo size={28} weight="fill" className="text-neutral-50" />
              <h3 className="mt-5 text-xl font-semibold text-neutral-50">Mac</h3>
              <p className="mt-2 flex-1 leading-relaxed text-neutral-400">
                Open Codync and it starts the host itself. Or install with Homebrew:
              </p>
              <div className="mt-5">
                <CopyCommand command="brew install --cask leepokai/codync/codync" platform="mac" />
              </div>
              <a href={DMG} className="mt-4 inline-flex h-11 items-center justify-center gap-2 rounded-full bg-neutral-50 px-5 font-medium text-neutral-950 transition hover:bg-white active:scale-[0.98]">
                <AppleLogo size={16} weight="fill" />
                Download for Mac
              </a>
            </div>
          </Reveal>

          <Reveal className="h-full" delay={0.05}>
            <div className="flex h-full flex-col rounded-3xl bg-neutral-900 p-8">
              <WindowsLogo size={28} weight="fill" className="text-neutral-50" />
              <h3 className="mt-5 text-xl font-semibold text-neutral-50">Windows</h3>
              <p className="mt-2 flex-1 leading-relaxed text-neutral-400">
                Run the installer and Codync starts the host itself. The installer isn&apos;t signed yet, so Windows
                asks you to confirm it once.
              </p>
              <a href={WINDOWS} className="mt-5 inline-flex h-11 items-center justify-center gap-2 rounded-full bg-neutral-50 px-5 font-medium text-neutral-950 transition hover:bg-white active:scale-[0.98]">
                <WindowsLogo size={16} weight="fill" />
                Download for PC
              </a>
            </div>
          </Reveal>

          <Reveal className="h-full" delay={0.1}>
            <div className="flex h-full flex-col rounded-3xl bg-neutral-900 p-8">
              <LinuxLogo size={28} weight="fill" className="text-neutral-50" />
              <h3 className="mt-5 text-xl font-semibold text-neutral-50">Linux and servers</h3>
              <p className="mt-2 flex-1 leading-relaxed text-neutral-400">
                The host, plus the desktop app when there is a display. Then run{" "}
                <code className="font-mono text-neutral-200">codync-host install</code>.
              </p>
              <div className="mt-5">
                <CopyCommand command={INSTALL_SH} platform="linux" />
              </div>
              <p className="mt-4 text-sm text-neutral-500">
                Desktop app:{" "}
                <a href={LINUX} className="text-neutral-300 underline underline-offset-4 transition hover:text-white">
                  x86_64
                </a>
                {" · "}
                <a href={LINUX_ARM} className="text-neutral-300 underline underline-offset-4 transition hover:text-white">
                  arm64
                </a>
              </p>
            </div>
          </Reveal>

          <Reveal className="h-full" delay={0.15}>
            <div className="flex h-full flex-col rounded-3xl bg-neutral-900 p-8">
              <DeviceMobile size={28} weight="fill" className="text-neutral-50" />
              <h3 className="mt-5 text-xl font-semibold text-neutral-50">iPhone</h3>
              <p className="mt-2 flex-1 leading-relaxed text-neutral-400">
                Get the app, then scan the code from <span className="text-neutral-200">Pair iPhone…</span> in the
                Mac menu bar, the Windows or Linux app, or <code className="font-mono text-neutral-200">codync-host pair</code>.
              </p>
              <a
                href={APP_STORE}
                target="_blank"
                rel="noopener noreferrer"
                className="mt-5 inline-flex w-fit transition hover:opacity-80 active:scale-[0.98]"
              >
                <Image src="/app-store-badge.svg" alt="Download on the App Store" width={120} height={40} className="h-11 w-auto" />
              </a>
            </div>
          </Reveal>
        </div>
      </div>
    </section>
  );
}
