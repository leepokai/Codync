import type { Metadata } from "next";
import { notFound } from "next/navigation";
import GitHubStarButton from "../../components/github-star-button";
import Halftone from "../../components/halftone";
import { SiteFooter, SiteHeader } from "../../components/site-chrome";
import { DMG_FILE, WINDOWS_FILE, WINDOWS_ARM64_FILE } from "../../links";

const files: Record<string, string> = { mac: DMG_FILE, windows: WINDOWS_FILE, "windows-arm64": WINDOWS_ARM64_FILE };

export const dynamicParams = false;

export function generateStaticParams() {
  return Object.keys(files).map((platform) => ({ platform }));
}

export const metadata: Metadata = { title: "Thanks for downloading", robots: { index: false } };

// The download starts by itself (meta refresh to the file keeps this page on screen) while we ask for a star.
export default async function DownloadPage({ params }: { params: Promise<{ platform: string }> }) {
  const { platform } = await params;
  const file = files[platform];
  if (!file) notFound();

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
          </p>
        </div>
      </main>
      <SiteFooter />
    </>
  );
}
