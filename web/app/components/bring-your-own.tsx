import { AgentPanel, AgentsPanel, PlanPanel, SandboxPanel } from "./byo-panels";
import Card from "./card";
import { CloudPanel } from "./cloud";
import Reveal from "./reveal";
import SectionHeader from "./section-header";

// Bring your own sandbox: where the bots run, which agents they use and who gets paid, one card each.
export default function BringYourOwn() {
  return (
    <section className="px-4 py-24 sm:px-6 md:py-32">
      <div className="mx-auto max-w-6xl">
        <SectionHeader label="Bring your own sandbox" title="Your computer, your agents, your plan.">
          Codync brings the bots. They run on a machine you choose, with the agents and subscriptions you already have.
        </SectionHeader>

        <div className="mt-14 grid grid-cols-1 gap-4 md:grid-cols-2 lg:grid-cols-3">
          <Reveal>
            <Card
              title="Your sandbox"
              body="Your Mac or PC, a Linux server over SSH, or a cloud computer on your own Boat or InstaBox account."
            >
              <SandboxPanel />
            </Card>
          </Reveal>
          <Reveal delay={0.05}>
            <Card title="Your agent" body="Claude Code, Codex and 40+ more over ACP, with the logins you already have. Each bot picks its own.">
              <AgentsPanel />
            </Card>
          </Reveal>
          <Reveal delay={0.1}>
            <Card
              id="cost"
              title="Your subscription"
              body="Bots run on the Claude, ChatGPT or other plans you already pay for. Codync has no paid tier."
              wellClassName="bg-neutral-50 p-5"
            >
              <PlanPanel />
            </Card>
          </Reveal>
          <Reveal className="lg:col-span-2">
            <Card
              title="Out of usage? Another agent carries on."
              body="When a bot's agent hits its limit, the next one in its list takes the same turn, with the same memory, folder and tools."
            >
              <AgentPanel />
            </Card>
          </Reveal>
          <Reveal delay={0.05}>
            <Card
              id="cloud"
              title="Codync Cloud (beta)"
              body="No computer on all day? Start one on your own Boat or InstaBox account; InstaBox's free plan includes one machine. It sleeps while your bots are idle."
            >
              <CloudPanel />
            </Card>
          </Reveal>
        </div>
      </div>
    </section>
  );
}
