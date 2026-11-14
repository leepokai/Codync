import Phone from "./phone";
import SectionHeader from "./section-header";
import StepScroller from "./step-scroller";

const steps = [
  {
    title: "Message anyone",
    body: "Each bot is a contact with a name, a face, an agent and a folder. You get its answer for every turn; tool calls and thoughts wait in the full conversation.",
    visual: <Phone src="/screens/answer.webp" alt="Scout's reply: it added weeklyTotals to src/pace.js with a test, and all tests pass" />,
  },
  {
    title: "Put a few in a room",
    body: "In a group chat every member answers in its own session. They read each other, disagree and hand work back. Mention one with @ to ask just that bot.",
    visual: <Phone src="/screens/group.webp" alt="Ship room group chat: Reviewer flags two issues and asks Scout, who adds a guard" />,
  },
  {
    title: "Approve from your phone",
    body: "Edits and commands arrive as cards: allow once, always allow or deny. A notification comes only when a bot needs you or finishes.",
    visual: <Phone src="/screens/approval.webp" alt="An approval card on the iPhone: a bot asks to edit src/pace.js, with Allow once, Always allow and Deny" />,
  },
  {
    title: "Glance without opening the app",
    body: "Widgets, Live Activities and the Dynamic Island show what your bots are doing on iPhone.",
    visual: <Phone src="/screens/activity.webp" alt="A Live Activity: Reviewer is running the test suite, with a thinking orb" />,
  },
];

// How working with the bots goes, step by step.
export default function Features() {
  return (
    <section className="px-4 pt-24 sm:px-6 md:pt-32">
      <div className="mx-auto max-w-6xl">
        <SectionHeader label="How it works" title="Message your bots like a team.">
          Talk to one bot, or put a few in a room. You stay in charge of what they do.
        </SectionHeader>
        <div className="mt-16 md:mt-0">
          <StepScroller steps={steps} />
        </div>
      </div>
    </section>
  );
}
