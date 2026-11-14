"use client";

import { useLayoutEffect, useRef, useState } from "react";

/** The narrowest layout the app gets; a smaller frame shows it scaled down (still interactive). */
const MIN_WIDTH = 960;
const RATIO = 1096 / 1600;

// The page is public/demo/desktop.html, served at /demo/desktop like every other page (Vercel
// serves clean URLs and no folder index.html); its assets live in public/demo/desktop/.
// The real desktop app (apps/desktop, `npm run build:demo`) on an in-memory host, in a Mac window.
// The app hides the title bar on macOS, so the window draws the traffic lights.
export default function MacWindow() {
  const frame = useRef<HTMLDivElement>(null);
  const [width, setWidth] = useState(0);
  useLayoutEffect(() => {
    const el = frame.current;
    if (!el) return;
    const ro = new ResizeObserver(() => setWidth(el.clientWidth));
    ro.observe(el);
    return () => ro.disconnect();
  }, []);
  const layout = Math.max(width, MIN_WIDTH);
  const scale = width ? width / layout : 1;

  return (
    <div
      ref={frame}
      className="relative aspect-[1600/1096] w-full overflow-hidden rounded-[12px] bg-[#0a0a0a] shadow-[0_40px_60px_rgba(0,0,0,0.8)] [color-scheme:dark]"
    >
      {width ? (
        <iframe
          src="/demo/desktop"
          title="Codync for Mac, live demo: bots on a MacBook Pro and a Mac mini, with Ship room open, where Pacer, Reviewer and Scout discuss an isoWeek fix"
          loading="lazy"
          className="absolute top-0 left-0 origin-top-left border-0"
          style={{ width: layout, height: layout * RATIO, transform: `scale(${scale})` }}
        />
      ) : null}
      <div aria-hidden className="pointer-events-none absolute flex gap-2" style={{ top: 16 * scale, left: 16 * scale, transform: `scale(${scale})`, transformOrigin: "top left" }}>
        <span className="size-3 rounded-full bg-[#ff5f57]" />
        <span className="size-3 rounded-full bg-[#febc2e]" />
        <span className="size-3 rounded-full bg-[#28c840]" />
      </div>
    </div>
  );
}
