// The user manual's pages, in sidebar order: each is content/docs/<slug>.mdx. The title is the page's heading,
// sidebar entry and tab title; the description is its meta description.
export type DocPage = { slug: string; title: string; description: string };

export const docSections: { title: string; pages: DocPage[] }[] = [
  {
    title: "Start",
    pages: [
      {
        slug: "getting-started",
        title: "Getting started",
        description: "Install Codync on your computer, pair your iPhone, create your first bot and send it a message.",
      },
    ],
  },
  {
    title: "Bots",
    pages: [
      {
        slug: "bots",
        title: "Bots",
        description: "What a Codync bot is, how to set one up, and how to organize, reset and delete bots.",
      },
      {
        slug: "agents",
        title: "Agents",
        description: "Which coding agents Codync runs, how it finds them, and how to install, sign in and pick models.",
      },
      {
        slug: "conversations",
        title: "Conversations",
        description:
          "Chatting with a bot: messages and files, approval cards, stopping, replies, reactions, commands, fresh conversations and notifications.",
      },
      {
        slug: "groups",
        title: "Group chats",
        description:
          "Put several bots in one chat. They answer in turn, read each other, and each works in its own session with its own tools.",
      },
      {
        slug: "teamwork",
        title: "Teamwork",
        description:
          "Bots can ask each other for help, hand work off, and, with Manage the team on, create and set up other bots.",
      },
      {
        slug: "cloud-assistants",
        title: "Cloud assistants",
        description:
          "Add Codync to ChatGPT, Claude or another assistant as a connector, so it can give your bots tasks and read their answers.",
      },
    ],
  },
  {
    title: "What bots can do",
    pages: [
      {
        slug: "routines",
        title: "Routines",
        description:
          "Have a bot do work on a schedule or when a webhook arrives: set it up in the routine editor or by asking the bot.",
      },
      {
        slug: "artifacts",
        title: "Artifacts and the Library",
        description:
          "Pages a bot makes for you to come back to, such as reports and trackers, and the Library of everything made or shared in a chat.",
      },
      {
        slug: "memory",
        title: "Memory",
        description: "What a bot remembers about you across sessions, how it updates that memory, and what the apps show.",
      },
      {
        slug: "connectors-and-skills",
        title: "Connectors and skills",
        description:
          "Give your bots tools through MCP connectors and reusable instructions through skills, installed once on your computer and switched on per bot.",
      },
      {
        slug: "agent-fallbacks",
        title: "Agent fallbacks",
        description:
          "Give a bot a list of other agents that carry on its work when its own agent runs out of usage, is rate limited, signed out or won't start.",
      },
      {
        slug: "computer-use",
        title: "Computer use and remote screen",
        description: "See and control your computer's screen from your iPhone, and let bots use the apps on your computer.",
      },
      {
        slug: "voice",
        title: "Voice",
        description:
          "Talk to a bot in a hands-free voice call, or dictate into the message box, with on-device speech or OpenAI and Gemini on your own API key.",
      },
    ],
  },
  {
    title: "Your computer",
    pages: [
      {
        slug: "remote-access",
        title: "Remote access",
        description:
          "How your phone reaches your computer: directly over Wi-Fi, Tailscale or your own URL, or through the Codync relay, always end-to-end encrypted.",
      },
      {
        slug: "linux-servers",
        title: "Linux servers",
        description:
          "Run the Codync host on a headless Linux server or cloud VM, pair your phone with it, and reach it from anywhere.",
      },
      {
        slug: "updates",
        title: "Updates",
        description:
          "How the desktop app, the host and the iPhone app update, what happens when the phone and computer versions differ, and how to uninstall Codync.",
      },
      {
        slug: "privacy",
        title: "Privacy",
        description:
          "What stays on your computer, what the Codync relay and push service see, and how usage analytics work and how to turn them off.",
      },
    ],
  },
];

export const docPages: DocPage[] = docSections.flatMap((s) => s.pages);
