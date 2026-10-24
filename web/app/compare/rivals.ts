// Comparison pages. Every claim about another product comes from the sources listed with it;
// anything those pages don't say is "not documented", never a guess. Re-check when updating.

type Row = [topic: string, codync: string, them: string];

export type Rival = {
  slug: string;
  name: string;
  /** One line on the card. */
  card: string;
  summary: string;
  rows: Row[];
  pickUs: string;
  pickThem: string;
  checked: string;
  sources: [label: string, url: string][];
};

const us = {
  price: "$0, every feature included. Apache 2.0 licensed.",
  runs: "On your own computer: a Mac, a Windows PC, a Linux desktop or a headless server.",
  agents:
    "Claude Code, Codex, Cursor, Gemini, Copilot, OpenCode and about 40 more over ACP, on the plans and logins you already have.",
  apps: "iPhone, Mac, Windows and Linux apps, plus a terminal UI over SSH.",
  data: "Stays on your computer. The phone connects directly, or through an end-to-end encrypted relay.",
  source: "Open source, Apache 2.0.",
  account: "Not required: pair by scanning a code. Signing in is optional.",
  approvals: "Edits and commands come to your phone: allow once, always allow or deny.",
  rooms: "Group chats where every bot answers in its own session, and replies that quote any message.",
  memory: "Memory per bot, and routines that run on a schedule or from a webhook.",
  tools:
    "Any connector from the MCP registry, your apps through Composio, and skills. Bots can also see and use the computer.",
  extras: "Voice calls with a bot, and remote screen to watch and control the computer from the phone.",
};

const CHECKED = "October 6, 2026";

export const rivals: Rival[] = [
  {
    slug: "grok-bot",
    name: "Grok Bot",
    card: "Same bots, rooms and approvals, but on your computer and free.",
    summary:
      "Grok Bot gives you named AI teammates that work on one shared cloud computer, with a paid Cursor or Grok plan. Codync is the same way of working, free and open source, with the bots on your own computer and any agent you already use.",
    rows: [
      ["Price", us.price, "No free plan listed. Comes with Cursor Pro ($20/mo) and up or SuperGrok ($30/mo) and up; usage beyond the plan is billed by tokens."],
      ["Where bots run", us.runs, "On one persistent cloud computer per user, run by SpaceXAI."],
      ["Agents and models", us.agents, "Its own bots; coding work goes to Cursor Cloud Agents. Bringing your own model isn't documented."],
      ["Apps", us.apps, "Desktop for macOS, Windows and Linux, an iPhone app, and team bots in Slack."],
      ["Your data", us.data, "On the cloud computer, encrypted in transit and at rest, with a training opt-out."],
      ["Source", us.source, "Not published as open source."],
      ["Account", us.account, "Required, with an eligible Cursor or Grok plan."],
      ["Approvals", us.approvals, "Yes, with Auto Review for sensitive actions."],
      ["Group chats and replies", us.rooms, "Yes: group chats and bot-to-bot threads."],
      ["Memory and routines", us.memory, "Yes: memory, scheduled routines and teaching a task."],
      ["Connected apps and tools", us.tools, "They sign in to your tools and use a browser on their cloud computer."],
      ["Voice and screen", us.extras, "Voice chat and voice memos. Bots use their own cloud computer with a browser."],
    ],
    pickUs:
      "you want Grok Bot's way of working without a subscription, with the bots on your own machine next to your files and tools, and the freedom to run Claude Code, Codex or any other agent you already pay for.",
    pickThem:
      "you'd rather have bots on a managed cloud computer that stays on when your machine is off, and you already pay for Cursor or SuperGrok.",
    checked: CHECKED,
    sources: [
      ["Grok Bot page and FAQ", "https://x.ai/bot"],
      ["Introducing Grok Bot", "https://x.ai/news/introducing-grok-bot"],
      ["Grok Bot changelog", "https://x.ai/changelog/bot"],
    ],
  },
  {
    slug: "muse",
    name: "Muse",
    card: "Meta's cloud agent and model, or bots on your own computer.",
    summary:
      "Muse is Meta's personal AI agent for goals and errands such as email, travel and shopping, running on a cloud VM with Meta's own model. Codync gives you a team of bots on your own computer, for code and everything else: they run the agents you choose, use your connected apps, browse and use the computer, and work on a schedule, for free.",
    rows: [
      ["Price", us.price, "Free with a usage limit. Power is $20/mo and Maximum $100/mo."],
      ["Where bots run", us.runs, "On Muse Secure VM, a cloud Linux VM run by Meta."],
      ["Agents and models", us.agents, "Meta's Muse Spark model only."],
      ["Apps", us.apps, "iPhone, Android, web, a Mac app and WhatsApp."],
      ["Your data", us.data, "On the cloud VM, not shared with Meta's ad systems, with a training opt-out."],
      ["Source", us.source, "Not published as open source."],
      ["Account", us.account, "Required: a Meta, Facebook or Instagram account, 18 or older."],
      ["Approvals", us.approvals, "Yes, before sensitive actions, with a full audit trail."],
      ["Group chats and replies", us.rooms, "Not documented."],
      ["Memory and routines", us.memory, "Memory you can tell to forget, and scheduled or event-driven background work."],
      ["Connected apps and tools", us.tools, "Email, travel and shopping; it uses a browser and terminal on its VM and can write its own tools."],
      ["Voice and screen", us.extras, "Not documented. The agent has a browser and terminal on its own VM."],
    ],
    pickUs:
      "you want your agents on your own computer with your files, the model and agent of your choice instead of one vendor's, and a team of named bots you can put in a room and message from your phone.",
    pickThem: "you want a personal assistant for email, travel and shopping inside Meta's apps, running in Meta's cloud with nothing to install.",
    checked: CHECKED,
    sources: [
      ["Introducing Muse (Meta)", "https://about.fb.com/news/2026/09/introducing-muse-personal-ai-agent/"],
      ["Muse", "https://muse.ai"],
      ["How we designed Muse", "https://introducing.muse.ai/"],
      ["Muse subscriptions", "https://www.meta.com/help/subscriptions/1021145227643680/"],
    ],
  },
  {
    slug: "dots",
    name: "Dots",
    card: "OpenAI's always-on cloud agent, or any agent on your computer.",
    summary:
      "Dots are OpenAI's always-on agents: one dot per person, on GPT-6 Astra, working from its own cloud computer, with a ChatGPT Pro or Business Premium plan. Codync gives you a team of bots on your own computer that run Claude Code, Codex or any other agent you already use, for free.",
    rows: [
      ["Price", us.price, "Your first dot comes with ChatGPT Pro (from $100/mo) or Business Premium; deeper work draws on a plan allowance."],
      ["Where bots run", us.runs, "On its own cloud computer and browser, run by OpenAI. It can use your laptop if you allow it."],
      ["Agents and models", us.agents, "GPT-6 Astra. A dot can start tasks in Codex."],
      ["Apps", us.apps, "ChatGPT on desktop, web and mobile, plus Slack and Teams."],
      ["Your data", us.data, "In OpenAI's cloud. Personal plans can opt out of training."],
      ["Source", us.source, "Not open source."],
      ["Account", us.account, "Required: ChatGPT Pro or Business Premium, 18 or older, in eligible markets."],
      ["Approvals", us.approvals, "Yes: auto-review, and Custom Rules to allow, require approval for or block actions."],
      ["Group chats and replies", us.rooms, "One primary dot per person for now; teams of dots are planned. It joins Slack and Teams conversations."],
      ["Memory and routines", us.memory, "Yes: memory, recurring tasks and background \"proactive research\"."],
      ["Connected apps and tools", us.tools, "Over 4,000 apps through OpenAI plugins, and its own browser."],
      ["Voice and screen", us.extras, "Voice calls. You can open its cloud computer to watch its work."],
    ],
    pickUs:
      "you want more than one bot, on your own machine next to your files, running Claude Code, Codex or any agent you choose, without a $100 a month plan.",
    pickThem: "you already pay for ChatGPT Pro or Business Premium and want one always-on assistant in OpenAI's cloud that also works in Slack and Teams.",
    checked: "October 7, 2026",
    sources: [
      ["Introducing dots (OpenAI)", "https://openai.com/index/introducing-dots/"],
      ["Getting started with your dot", "https://help.openai.com/en/articles/20001530-getting-started-with-your-dot"],
      ["ChatGPT pricing", "https://chatgpt.com/pricing"],
    ],
  },
  {
    slug: "claude-remote-control",
    name: "Claude Remote Control",
    card: "Anthropic's phone remote is Claude only. Codync runs any agent.",
    summary:
      "Remote Control lets the Claude app on your phone drive a Claude Code session on your computer, with a paid Claude plan. Codync works with Claude Code and every other agent, as persistent bots you can put in a room, for free.",
    rows: [
      ["Price", us.price, "Needs a Claude Pro, Max, Team or Enterprise plan; API keys and other providers don't work."],
      ["Where bots run", us.runs, "Claude Code on your machine, relayed through Anthropic; or cloud sessions on Anthropic's servers."],
      ["Agents and models", us.agents, "Claude Code only."],
      ["Apps", us.apps, "The Claude app on iPhone, Android and iPad, and claude.ai/code on the web."],
      ["Your data", us.data, "Relayed through the Anthropic API over TLS; end-to-end encryption isn't documented."],
      ["Source", us.source, "Not open source."],
      ["Account", us.account, "Required: a claude.ai account."],
      ["Approvals", us.approvals, "Yes, from the phone."],
      ["Group chats and replies", us.rooms, "No: up to 32 parallel sessions, not named bots."],
      ["Memory and routines", us.memory, "Not documented for Remote Control."],
      ["Connected apps and tools", us.tools, "Not documented for Remote Control."],
      ["Voice and screen", us.extras, "Not documented."],
    ],
    pickUs:
      "you use more than Claude Code, want persistent named bots that remember and can work together, or want your phone's connection end-to-end encrypted.",
    pickThem: "you only use Claude Code on a Claude plan and want Anthropic's official app, including cloud sessions.",
    checked: CHECKED,
    sources: [
      ["Remote Control", "https://code.claude.com/docs/en/remote-control"],
      ["Claude Code on mobile", "https://code.claude.com/docs/en/mobile"],
    ],
  },
  {
    slug: "happy",
    name: "Happy",
    card: "Both free and open source. Codync adds bots, rooms and memory.",
    summary:
      "Happy is a free, open-source phone and web client for Claude Code and Codex sessions on your computer. Codync turns those agents into persistent named bots with group chats, memory, routines, voice and remote screen.",
    rows: [
      ["Price", us.price, "Free."],
      ["Where bots run", us.runs, "On your computer; you start sessions with happy claude instead of claude."],
      ["Agents and models", us.agents, "Claude Code and Codex."],
      ["Apps", us.apps, "iPhone, Android, web and a Mac app."],
      ["Your data", us.data, "End-to-end encrypted through a relay you can host yourself."],
      ["Source", us.source, "Open source, MIT."],
      ["Account", us.account, "No sign-up: pair by key."],
      ["Approvals", us.approvals, "Push notifications when a permission is needed."],
      ["Group chats and replies", us.rooms, "No: parallel sessions, not named bots."],
      ["Memory and routines", us.memory, "Not documented."],
      ["Connected apps and tools", us.tools, "Not documented."],
      ["Voice and screen", us.extras, "A voice agent. No remote screen."],
    ],
    pickUs:
      "you want bots that keep going between sessions: named, with memory and routines, working together in group chats, and reachable by voice or remote screen.",
    pickThem: "you want a thin remote for Claude Code or Codex sessions you start yourself, or you're on Android.",
    checked: CHECKED,
    sources: [
      ["Happy", "https://happy.engineering/"],
      ["Happy FAQ", "https://happy.engineering/docs/faq/"],
      ["How it works", "https://happy.engineering/docs/how-it-works/"],
      ["GitHub", "https://github.com/slopus/happy"],
    ],
  },
];
