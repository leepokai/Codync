"use client";

import { useEffect } from "react";
import posthog from "posthog-js";
import "posthog-js/dist/web-vitals"; // Bundled, as external loading is off below.
import { ANDROID_TEST, APP_STORE, CLOUD_SECTION, DISCORD, DMG, GITHUB, KO_FI, LARK_JOIN, LARK_QR, LINUX, LINUX_ARM, PRODUCT_HUNT, WECHAT_QR, WINDOWS } from "./links";

// Cookieless counts for codync.dev (docs/reference/analytics-events.md, Website). Nothing is
// stored on the device; PostHog's server hashes the visitor per day.
const TOKEN = "phc_rQHvxdSFKnMquPfMjmiMhXhM4tqJ5qCYsWuVEoFxCCWX"; // write-only project token
const LOCAL = ["localhost", "127.0.0.1", "[::1]"];
const MAIN = ["codync.dev", "www.codync.dev"];

export type Platform = "mac" | "windows" | "linux" | "ios";

const downloads: Record<string, Platform> = { [DMG]: "mac", [WINDOWS]: "windows", [LINUX]: "linux", [LINUX_ARM]: "linux", [APP_STORE]: "ios" };
// Matched by prefix, first hit wins.
const links: [prefix: string, link: string][] = [
  [`${GITHUB}/issues`, "issues"],
  [GITHUB, "github"],
  [DISCORD, "discord"],
  [LARK_JOIN, "lark"],
  [LARK_QR, "lark_qr"],
  [WECHAT_QR, "wechat_qr"],
  [PRODUCT_HUNT, "product_hunt"],
  [KO_FI, "ko_fi"],
  [ANDROID_TEST, "android_test"],
  [CLOUD_SECTION, "cloud_new"],
];

export function track(event: "download_clicked", props: { platform: Platform }): void;
export function track(event: "link_opened", props: { link: string }): void;
export function track(event: string, props: Record<string, string>) {
  // A beacon leaves before the click navigates away.
  if (posthog.__loaded) posthog.capture(event, props, { transport: "sendBeacon" });
}

// Every download and outbound link is a plain <a>, so one listener covers them all.
function onClick(e: MouseEvent) {
  const href = (e.target as Element).closest?.("a[href]")?.getAttribute("href");
  if (!href) return;
  const platform = downloads[href];
  if (platform) return track("download_clicked", { platform });
  const link = links.find(([prefix]) => href.startsWith(prefix))?.[1];
  if (link) track("link_opened", { link });
}

// A click into the YouTube player moves focus into its cross-origin iframe.
function onBlur() {
  if ((document.activeElement as HTMLIFrameElement | null)?.src?.includes("youtube")) {
    track("link_opened", { link: "demo" });
  }
}

export default function Analytics() {
  useEffect(() => {
    const host = location.hostname;
    if (LOCAL.includes(host)) return;
    posthog.init(TOKEN, {
      api_host: "/e", // vercel.json rewrites it to PostHog.
      cookieless_mode: "always",
      capture_pageview: "history_change",
      capture_pageleave: true,
      autocapture: false,
      rageclick: false,
      capture_dead_clicks: false,
      capture_heatmaps: false,
      capture_performance: { web_vitals: true, web_vitals_attribution: false },
      capture_exceptions: false,
      disable_session_recording: true,
      disable_surveys: true,
      disable_external_dependency_loading: true,
      advanced_disable_flags: true,
      loaded: (ph) => ph.register({ env: MAIN.includes(host) ? "main" : "dev" }),
    });
    document.addEventListener("click", onClick, true);
    window.addEventListener("blur", onBlur);
    return () => {
      document.removeEventListener("click", onClick, true);
      window.removeEventListener("blur", onBlur);
    };
  }, []);
  return null;
}
