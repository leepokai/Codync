import { ArrowsClockwise, SunHorizon, WebhooksLogo } from "@phosphor-icons/react/ssr";
import Card from "./card";
import LivePhones from "./live-phone/live-phones";
import { NetworkPanel } from "./network";
import Reveal from "./reveal";
import SectionHeader from "./section-header";

// A bot's routines, drawn like its Routines list (docs/features/routines.md).
const routines = [
  { icon: SunHorizon, name: "Morning brief", when: "Every day, 08:00" },
  { icon: ArrowsClockwise, name: "Check the build", when: "Every hour" },
  { icon: WebhooksLogo, name: "Deploy finished", when: "Webhook" },
];

// What keeps happening while you're away from the computer.
export default function Away() {
  return (
    <section className="px-4 py-24 sm:px-6 md:py-32">
      <div className="mx-auto max-w-6xl">
        <SectionHeader label="While you're away" title="Your bots keep working when you step out.">
          They run on your computer, not your phone. Check in, approve and get the result from anywhere.
        </SectionHeader>

        <div className="mt-14 grid grid-cols-1 gap-4 md:grid-cols-2">
          <Reveal className="md:row-span-2">
            <Card
              title="Answer from your phone"
              body="Pair your phone, then reply and approve from anywhere: on the same Wi-Fi, over Tailscale or through the encrypted relay."
              wellClassName="bg-neutral-950 px-6 pt-8 pb-20"
            >
              {/* The live iPhones; the front one hangs below the back one. */}
              <div className="relative mx-auto w-full max-w-[24rem]">
                <LivePhones />
              </div>
            </Card>
          </Reveal>
          <Reveal delay={0.05}>
            <Card title="On a schedule" body="Routines run a bot every morning, every hour or when a webhook arrives, and the result comes as a message.">
              <ul className="flex flex-col gap-1">
                {routines.map(({ icon: Icon, name, when }) => (
                  <li key={name} className="flex items-center gap-3 rounded-2xl bg-neutral-900 px-4 py-3">
                    <Icon size={18} className="shrink-0 text-neutral-50" />
                    <span className="flex-1 text-sm font-medium text-neutral-50">{name}</span>
                    <span className="text-sm text-neutral-500">{when}</span>
                  </li>
                ))}
              </ul>
            </Card>
          </Reveal>
          <Reveal delay={0.1}>
            <Card
              id="network"
              title="Bots on your other computers (beta)"
              body="Put your computers in a private network and a bot on one can ask a bot on another. They connect peer to peer."
            >
              <NetworkPanel />
            </Card>
          </Reveal>
        </div>
      </div>
    </section>
  );
}
