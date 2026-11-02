import { useSyncExternalStore } from "react";

// The computer a visitor is on, as far as the browser says: one installer each, `linux-arm64` for Arm Linux.
// Phones and anything unknown (Android, ChromeOS, 32-bit Arm) get the Mac default, as before.
// Chromium reports every Linux as x86_64, so Arm Linux is only caught on Firefox; the menu lists both.
export type Os = "mac" | "windows" | "linux" | "linux-arm64" | "ios";

function detect(): Os {
  const ua = navigator.userAgent;
  if (/android|cros/i.test(ua)) return "mac";
  // iPadOS Safari presents itself as a Mac, but a Mac has no touch screen.
  if (/iphone|ipad|ipod/i.test(ua) || (/macintosh/i.test(ua) && navigator.maxTouchPoints > 1)) return "ios";
  if (/windows/i.test(ua)) return "windows";
  if (/linux|x11/i.test(ua)) {
    if (/aarch64|arm64/i.test(ua)) return "linux-arm64";
    return /arm/i.test(ua) ? "mac" : "linux"; // 32-bit Arm has no build
  }
  return "mac";
}

const never = () => () => {};

// Server and first client render say "mac"; the visitor's own OS replaces it right after hydration.
export function useOs(): Os {
  return useSyncExternalStore(never, detect, () => "mac");
}
