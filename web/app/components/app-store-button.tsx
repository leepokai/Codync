import { APP_STORE } from "../links";

// The App Store badge's "Download on the / App Store" lockup, sized like the other pill buttons.
export default function AppStoreButton() {
  return (
    <a
      href={APP_STORE}
      target="_blank"
      rel="noopener noreferrer"
      aria-label="Download on the App Store"
      className="inline-flex h-12 items-center gap-2.5 rounded-full bg-neutral-900 pr-6 pl-5 text-neutral-100 transition hover:bg-neutral-800 active:scale-[0.98]"
    >
      {/* eslint-disable-next-line @next/next/no-img-element -- Apple's App Store icon (Wikimedia Commons) */}
      <img src="/app-store-icon.svg" alt="" width={28} height={28} className="size-7" />
      <span className="flex flex-col leading-none">
        <span className="text-[11px] font-medium text-neutral-400">Download on the</span>
        <span className="mt-0.5 text-lg font-semibold tracking-tight">App Store</span>
      </span>
    </a>
  );
}

