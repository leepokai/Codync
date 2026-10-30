import { CaretDown, DeviceMobile, LinuxLogo, Terminal, WindowsLogo } from "@phosphor-icons/react/ssr";
import { AppleLogo } from "./apple-logo";
import { APP_STORE, DMG, WINDOWS, WINDOWS_ARM64 } from "../links";

const others: [label: string, detail: string, href: string, Icon: typeof DeviceMobile | typeof AppleLogo][] = [
  ["macOS", "Apple silicon and Intel", DMG, AppleLogo],
  ["Windows x64", "Installer for Intel and AMD PCs", WINDOWS, WindowsLogo],
  ["Windows ARM64", "Installer for ARM64 Windows 11", WINDOWS_ARM64, WindowsLogo],
  ["iPhone", "App Store", APP_STORE, DeviceMobile],
  ["Linux", "AppImage, deb and servers", "/#install", LinuxLogo],
  ["Terminal", "Over SSH, any machine", "/#install", Terminal],
];

// "Download for macOS" with a split menu for the other platforms; a native <details>, no script.
export default function DownloadButton({ size = "lg" }: { size?: "sm" | "lg" }) {
  const h = size === "sm" ? "h-9 text-sm" : "h-12";
  return (
    <div className="relative flex">
      <a
        href={DMG}
        className={`${h} inline-flex items-center gap-2 rounded-l-full bg-neutral-50 pr-4 pl-5 font-medium whitespace-nowrap text-neutral-950 transition hover:bg-white active:scale-[0.98]`}
      >
        <AppleLogo size={size === "sm" ? 15 : 18} weight="fill" />
        {size === "sm" ? "Download" : "Download for macOS"}
      </a>
      <details className="group">
        <summary
          aria-label="Other platforms"
          className={`${h} flex cursor-pointer list-none items-center rounded-r-full border-l border-neutral-300 bg-neutral-50 px-3 text-neutral-950 transition hover:bg-white [&::-webkit-details-marker]:hidden`}
        >
          <CaretDown size={14} weight="bold" className="transition-transform group-open:rotate-180" />
        </summary>
        <ul className="absolute right-0 z-40 mt-2 w-64 rounded-2xl bg-neutral-900 p-2 shadow-[0_20px_40px_rgba(0,0,0,0.6)]">
          {others.map(([label, detail, href, Icon]) => (
            <li key={label}>
              <a href={href} className="flex items-center gap-3 rounded-xl px-3 py-2.5 transition hover:bg-neutral-800">
                <Icon size={18} weight="fill" className="text-neutral-200" />
                <span>
                  <span className="block text-sm font-medium text-neutral-100">{label}</span>
                  <span className="block text-xs text-neutral-500">{detail}</span>
                </span>
              </a>
            </li>
          ))}
        </ul>
      </details>
    </div>
  );
}
