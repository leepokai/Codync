// Pages for people searching how to run one agent from their phone ("Claude Code app",
// "Codex mobile"). Facts about Codync only; keep them in step with the README.

export type AgentPage = {
  slug: string;
  agent: string;
  title: string;
  description: string;
  lead: string;
  /** Who signs the agent in, in the agent's own words. */
  login: string;
  /** Where the usage limits come from. */
  usage: string;
  faq: [question: string, answer: string][];
};

export const agentPages: AgentPage[] = [
  {
    slug: "claude-code-iphone",
    agent: "Claude Code",
    title: "Claude Code on iPhone, free",
    description:
      "Run Claude Code from your iPhone with Codync, a 100% free, open-source app. Claude Code keeps running on your own Mac, Windows PC or Linux machine; you chat, approve edits and see your Claude limits from the phone.",
    lead: "Claude Code keeps running on your own Mac, Windows PC or Linux machine, with the Claude login it already has. Codync turns it into named bots you message, approve and follow from your iPhone, for free.",
    login: "Claude Code signs in with your Claude plan on the computer, as it always does. Codync never sees or calls the Claude API with it.",
    usage: "Your Claude 5-hour and weekly limits, read locally from Claude Code, show on the phone, in widgets and in the Mac menu bar.",
    faq: [
      [
        "Is there a Claude Code app for iPhone?",
        "Codync is a free iPhone app for Claude Code: it runs Claude Code on your computer and lets you chat with it, approve its edits and commands, and get notified when it needs you or finishes.",
      ],
      [
        "Does it work with my Claude Pro or Max plan?",
        "Yes. Codync runs the Claude Code you already signed in to on your computer, so it uses whatever plan or key Claude Code uses there.",
      ],
      [
        "Can I use Claude Code and Codex together?",
        "Yes. Each bot picks its own agent, and a group chat can mix them: a Claude Code bot and a Codex bot can review each other's work in the same room.",
      ],
      [
        "What do I need on the computer?",
        "A Mac, Windows PC or Linux machine with Claude Code installed and Node.js (Codync starts Claude Code through its ACP adapter with npx).",
      ],
    ],
  },
  {
    slug: "codex-iphone",
    agent: "Codex",
    title: "Codex on iPhone, free",
    description:
      "Run OpenAI Codex from your iPhone with Codync, a 100% free, open-source app. Codex keeps running on your own Mac, Windows PC or Linux machine; you chat, approve commands and see your Codex limits from the phone.",
    lead: "Codex keeps running on your own Mac, Windows PC or Linux machine, with the ChatGPT login it already has. Codync turns it into named bots you message, approve and follow from your iPhone, for free.",
    login: "Codex signs in with your ChatGPT plan on the computer, as it always does. Codync never sees or calls the OpenAI API with it.",
    usage: "Your Codex 5-hour and weekly limits, read on your computer through the Codex CLI, show on the phone, in widgets and in the Mac menu bar.",
    faq: [
      [
        "Can I use Codex on my iPhone?",
        "Yes. Codync is a free iPhone app that runs Codex on your computer and lets you chat with it, approve its commands and edits, and get notified when it needs you or finishes.",
      ],
      [
        "Does it work with my ChatGPT plan?",
        "Yes. Codync runs the Codex you already signed in to on your computer, so it uses whatever plan or key Codex uses there.",
      ],
      [
        "Can I use Codex and Claude Code together?",
        "Yes. Each bot picks its own agent, and a group chat can mix them: a Codex bot and a Claude Code bot can review each other's work in the same room.",
      ],
      [
        "What do I need on the computer?",
        "A Mac, Windows PC or Linux machine with Codex installed and Node.js (Codync starts Codex through its ACP adapter with npx).",
      ],
    ],
  },
];
