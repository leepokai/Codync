// The tools the cloud assistant connector lists (`assistant.ts`): the messaging tools, and the worker tools each
// computer answers (the host's `chat::team::control::schema`, whose test checks the two name the same tools).

/** Who a call is: the account, and the assistant app (its OAuth client) it came through. */
export type Assistant = { user: string; client: string };

const COMPUTER = {
  type: "string",
  description: "Which of the user's computers, by name from list_computers. Needed only when several are online.",
};

const TOOLS = [
  {
    name: "list_computers",
    title: "List computers",
    description: "The computers in the user's Codync account, and whether each is online.",
    inputSchema: { type: "object", properties: {} },
    annotations: { readOnlyHint: true, openWorldHint: false },
  },
  {
    name: "list_bots",
    title: "List bots",
    description:
      "The user's Codync bots you can message, with their IDs, kinds, specialties and status, also those on the user's " +
      "other computers in that computer's network (IDs like reviewer@laptop), where Claude Code sessions and Codex " +
      "threads can be offered too. Narrow it with query or kind; it returns at most limit at a time, with next for " +
      "the rest. Without computer, every online computer answers. Rows without kind come from a computer whose " +
      "Codync is older and doesn't filter by kind.",
    inputSchema: {
      type: "object",
      properties: {
        query: { type: "string", description: "Words that must all appear in the bot's name or description." },
        kind: {
          type: "string",
          enum: ["codync", "claude-code", "codex", "terminal", "other"],
          description:
            "Only this kind of agent: codync (Codync bots), claude-code (Claude Code sessions), codex (Codex threads), " +
            "terminal (a person at a terminal on another computer) or other (on a computer whose Codync doesn't say " +
            "its kind yet).",
        },
        limit: { type: "integer", minimum: 1, maximum: 50, description: "At most this many (default 20)." },
        cursor: { type: "string", description: "next from the previous call, for the rest." },
        computer: COMPUTER,
      },
    },
    annotations: { readOnlyHint: true, openWorldHint: false },
  },
  {
    name: "message_bot",
    title: "Message a bot",
    description:
      "Send a self-contained request to one of the user's Codync bots. It runs in that bot's own chat on the " +
      "user's computer, with the bot's tools, folder and permissions, and this returns as soon as it is queued. " +
      "Get the reply later with read_replies.",
    inputSchema: {
      type: "object",
      properties: {
        botId: {
          type: "string",
          description: "The bot's id from list_bots; with computer set to the computer that listed it when several are online.",
        },
        message: { type: "string", description: "The task, relevant context, file paths and the expected result." },
        computer: COMPUTER,
      },
      required: ["botId", "message"],
    },
    annotations: { readOnlyHint: false, idempotentHint: false, openWorldHint: false },
  },
  {
    name: "read_replies",
    title: "Read replies",
    description:
      "Your recent message_bot requests, newest first, with each bot's reply once it's done. status is queued, " +
      "sent (running), completed, failed or cancelled. Give requestId for one request.",
    inputSchema: {
      type: "object",
      properties: {
        requestId: { type: "string", description: "One request's id from message_bot." },
        computer: COMPUTER,
      },
    },
    annotations: { readOnlyHint: true, openWorldHint: false },
  },
];

const THREAD = {
  botId: { type: "string", description: "botId from thread_list or thread_launch." },
  taskId: { type: "string", description: "taskId from thread_list or thread_launch." },
  computer: COMPUTER,
};
const LIMIT = {
  type: "string",
  enum: ["readOnly", "ask", "auto", "full"],
  description: "Act at most at this level for this call: it can narrow what the user gave cloud assistants, never widen it.",
};
const READS = { readOnlyHint: true, openWorldHint: false };
const WRITES = { readOnlyHint: false, idempotentHint: false, openWorldHint: false };

/** The worker tools: the computer answers each one (the host's `chat::team::control`, which describes them the same way). */
const WORKER_TOOLS = [
  {
    name: "worker_capabilities",
    title: "What you may do",
    description:
      "What the user lets cloud assistants do on that computer (read only unless they raised it there): your level, the bots " +
      "and folders you may use, whether you may start threads without a bot, its coding agents, the approval levels and limits.",
    inputSchema: { type: "object", properties: { computer: COMPUTER } },
    annotations: READS,
  },
  {
    name: "thread_list",
    title: "List threads",
    description:
      "Threads you may see, newest work first, with botId, taskId, title, status (waiting, working, needsYou, done, failed, " +
      "stopped), activity and a result excerpt. Pass next as cursor for older ones. Without computer, every online computer answers.",
    inputSchema: {
      type: "object",
      properties: {
        botId: { type: "string" },
        status: { type: "string", enum: ["waiting", "working", "needsYou", "done", "failed", "stopped"] },
        limit: { type: "integer", minimum: 1, maximum: 50 },
        cursor: { type: "string" },
        computer: COMPUTER,
      },
    },
    annotations: READS,
  },
  {
    name: "thread_launch",
    title: "Start a thread",
    description:
      "Start a coding agent on your message, on one of the user's bots (botId) or without a bot on an agent there (agent). " +
      "It returns at once: follow it with thread_wait and thread_read. approval: ask (default, every request is an approval " +
      "card on the user's devices), auto (the agent's own auto mode) or full; never more than worker_capabilities allows. " +
      "Approvals are never answered here. Give clientRequestId so a retry never starts a second thread.",
    inputSchema: {
      type: "object",
      properties: {
        message: { type: "string", description: "The task, relevant context, file paths and the expected result." },
        botId: { type: "string" },
        agent: { type: "string", description: "An agent id from worker_capabilities (needed without a bot)." },
        folder: { type: "string", description: "Full path: one of the bot's projects or a folder worker_capabilities lists." },
        place: { type: "string", enum: ["here", "worktree"] },
        model: { type: "string" },
        approval: { type: "string", enum: ["ask", "auto", "full"] },
        loginProfile: { type: "string" },
        clientRequestId: { type: "string" },
        accessLimit: LIMIT,
        computer: COMPUTER,
      },
      required: ["message"],
    },
    annotations: WRITES,
  },
  {
    name: "thread_send",
    title: "Write to a thread",
    description: "Write to a thread as a message in it: it waits behind a running turn (queue), or steer hands it to that turn now.",
    inputSchema: {
      type: "object",
      properties: { ...THREAD, message: { type: "string" }, mode: { type: "string", enum: ["queue", "steer"] }, clientRequestId: { type: "string" }, accessLimit: LIMIT },
      required: ["botId", "taskId", "message"],
    },
    annotations: WRITES,
  },
  {
    name: "thread_wait",
    title: "Wait on a thread",
    description:
      "Wait up to 8 seconds for a thread to change (with cursor) or to end or need the user (without). Returns status, " +
      "activity, needsYou, the result once it ended, and cursor; on timedOut call it again later with the cursor.",
    inputSchema: {
      type: "object",
      properties: { ...THREAD, timeoutSecs: { type: "integer", minimum: 0, maximum: 8 }, cursor: { type: "string" } },
      required: ["botId", "taskId"],
    },
    annotations: READS,
  },
  {
    name: "thread_read",
    title: "Read a thread",
    description: "Read a thread: view summary (status, result, last reply) or steps (its transcript, a page at a time).",
    inputSchema: {
      type: "object",
      properties: {
        ...THREAD,
        view: { type: "string", enum: ["summary", "steps"] },
        before: { type: "integer" },
        limit: { type: "integer", minimum: 1, maximum: 100 },
        maxCharsPerItem: { type: "integer", minimum: 100, maximum: 4000 },
      },
      required: ["botId", "taskId"],
    },
    annotations: READS,
  },
  {
    name: "thread_steer",
    title: "Steer a thread",
    description: "first (front of the queue), stop (end it), retry (a failed or stopped one) or drop (take it away). A routine's run or another bot's request is only ever moved first.",
    inputSchema: {
      type: "object",
      properties: { ...THREAD, how: { type: "string", enum: ["first", "stop", "retry", "drop"] }, accessLimit: LIMIT },
      required: ["botId", "taskId", "how"],
    },
    annotations: WRITES,
  },
  {
    name: "thread_interrupt",
    title: "Interrupt a thread",
    description: "Stop the turn a thread is running now.",
    inputSchema: { type: "object", properties: { ...THREAD, accessLimit: LIMIT }, required: ["botId", "taskId"] },
    annotations: WRITES,
  },
  {
    name: "thread_pending",
    title: "What waits on the user",
    description:
      "List (or read) what waits on the user in a thread. Approval requests are shown only so you can tell the user: they " +
      "are answered on the user's devices. A choice the thread asked (kind decision) can be answered with action answer.",
    inputSchema: {
      type: "object",
      properties: { ...THREAD, action: { type: "string", enum: ["list", "read", "answer"] }, requestId: { type: "string" }, option: { type: "string" }, accessLimit: LIMIT },
      required: ["botId", "taskId"],
    },
    annotations: WRITES,
  },
  {
    name: "routine_list",
    title: "List routines",
    description: "The routines of the bots you may use, with their recent runs.",
    inputSchema: { type: "object", properties: { botId: { type: "string" }, computer: COMPUTER } },
    annotations: READS,
  },
  {
    name: "routine_run",
    title: "Run a routine",
    description: "Run one of a bot's routines now (full access only).",
    inputSchema: {
      type: "object",
      properties: { botId: { type: "string" }, routineId: { type: "string" }, clientRequestId: { type: "string" }, accessLimit: LIMIT, computer: COMPUTER },
      required: ["botId", "routineId"],
    },
    annotations: WRITES,
  },
  {
    name: "workflow_list",
    title: "List workflows",
    description: "The workflows on that computer.",
    inputSchema: { type: "object", properties: { computer: COMPUTER } },
    annotations: READS,
  },
  {
    name: "workflow_run",
    title: "Run a workflow",
    description: "Start a workflow (full access only).",
    inputSchema: {
      type: "object",
      properties: { workflowId: { type: "string" }, input: { type: "string" }, clientRequestId: { type: "string" }, accessLimit: LIMIT, computer: COMPUTER },
      required: ["workflowId"],
    },
    annotations: WRITES,
  },
];

/** Tools every online computer answers when none is named: only the ones that read. Every other needs one computer. */
export const FAN_OUT = new Set(["list_bots", "read_replies", "worker_capabilities", "thread_list", "routine_list", "workflow_list"]);

export const ALL_TOOLS = [...TOOLS, ...WORKER_TOOLS];
