import type { Metadata } from "next";
import { notFound } from "next/navigation";
import CopyCommand from "../../components/copy-command";
import GitHubStarButton from "../../components/github-star-button";
import Halftone from "../../components/halftone";
import { SiteFooter, SiteHeader } from "../../components/site-chrome";
import {
  DMG_FILE,
  INSTALL_SH,
  LINUX_ARM_DEB_FILE,
  LINUX_ARM_FILE,
  LINUX_DEB_FILE,
  LINUX_FILE,
  WINDOWS_FILE,
} from "../../links";

// `deb` is a second package for the same computer, offered under the main file; Linux pages (`host`) also explain the host.
const files: Record<string, { file: string; deb?: string; host?: boolean }> = {
  mac: { file: DMG_FILE },
  windows: { file: WINDOWS_FILE },
  linux: { file: LINUX_FILE, deb: LINUX_DEB_FILE, host: true },
  "linux-arm64": { file: LINUX_ARM_FILE, deb: LINUX_ARM_DEB_FILE, host: true },
};

export const dynamicParams = false;

export function generateStaticParams() {
  return Object.keys(files).map((platform) => ({ platform }));
}

export const metadata: Metadata = { title: "Thanks for downloading", robots: { index: false } };

// The download starts by itself (meta refresh to the file keeps this page on screen) while we ask for a star.
export default async function DownloadPage({ params }: { params: Promise<{ platform: string }> }) {
  const { platform } = await params;
  const entry = files[platform];
  if (!entry) notFound();
  const { file, deb, host } = entry;

  return (
    <>
      <meta httpEquiv="refresh" content={`1;url=${file}`} />
      <SiteHeader />
      <main className="relative flex flex-1 items-center overflow-hidden px-4 py-24 sm:px-6">
        <Halftone corner="top-right" className="w-[36rem] max-w-[80vw] text-neutral-50 opacity-[0.14]" />
        <div className="relative mx-auto flex max-w-xl flex-col items-center text-center">
          <p className="text-sm text-neutral-500">Your download is starting…</p>
          <h1 className="mt-4 text-4xl font-semibold tracking-tighter text-neutral-50 md:text-5xl">
            Enjoying Codync? Give it a star.
          </h1>
          <p className="mt-4 text-neutral-400">
            Codync is free and open source. A star on GitHub is the easiest way to help other people find it.
          </p>
          <div className="mt-8">
            <GitHubStarButton />
          </div>
          <p className="mt-10 text-sm text-neutral-500">
            Download didn&apos;t start?{" "}
            <a href={file} className="text-neutral-300 underline underline-offset-4 transition hover:text-white">
              Try again
            </a>
            {deb && (
              <>
                {" · "}
                <a href={deb} className="text-neutral-300 underline underline-offset-4 transition hover:text-white">
                  Prefer a .deb?
                </a>
              </>
            )}
          </p>
          {host && (
            <div className="mt-8 w-full max-w-lg text-left">
              <p className="text-sm text-neutral-500">
                On Linux the desktop app doesn&apos;t include the host. Install it too with this command, and if the
                AppImage won&apos;t start on Ubuntu, run <code className="text-neutral-300">sudo apt install libfuse2t64</code>.
              </p>
              <div className="mt-3">
                <CopyCommand command={INSTALL_SH} platform="linux" />
              </div>
            </div>
          )}
        </div>
      </main>
      <SiteFooter />
    </>
  );
}
