import { ANDROID_TEST, APP_STORE } from "../links";

type Store = { href: string; icon: string; label: string; top: string; name: string };

const APPLE: Store = { href: APP_STORE, icon: "/app-store-icon.svg", label: "Download on the App Store", top: "Download on the", name: "App Store" };
// Android is in closed testing: the Google Play button opens the tester sign-up form.
const GOOGLE: Store = { href: ANDROID_TEST, icon: "/google-play-icon.svg", label: "Join the Android test on Google Play", top: "Get it on", name: "Google Play" };

// A store badge's "Download on the / App Store" lockup, sized like the other pill buttons.
function StoreButton({ href, icon, label, top, name }: Store) {
  return (
    <a
      href={href}
      target="_blank"
      rel="noopener noreferrer"
      aria-label={label}
      className="inline-flex h-12 items-center gap-2.5 rounded-full bg-neutral-900 pr-6 pl-5 text-neutral-100 transition hover:bg-neutral-800 active:scale-[0.98]"
    >
      {/* eslint-disable-next-line @next/next/no-img-element -- store icons from Wikimedia Commons */}
      <img src={icon} alt="" width={28} height={28} className="size-7" />
      <span className="flex flex-col leading-none">
        <span className="text-[11px] font-medium text-neutral-400">{top}</span>
        <span className="mt-0.5 text-lg font-semibold tracking-tight">{name}</span>
      </span>
    </a>
  );
}

export const AppStoreButton = () => <StoreButton {...APPLE} />;
export const PlayStoreButton = () => <StoreButton {...GOOGLE} />;
