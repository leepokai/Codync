// The scripted model every adapter talks to: one turn that runs a shell command through the agent's own shell tool,
// so a check sees the adapter's tool call, its permission card and its tool result reach the transcript. Whatever
// wire the agent speaks (Anthropic Messages, OpenAI Responses, Chat Completions, Gemini), aimock hands this the
// request in one shape: the turn's user message, the tools offered and the tool results so far.

export const MARK = "CODYNC-SCENARIO";
export const PROMPT = `${MARK}: run the shell command that writes beta into notes.txt`;
export const COMMAND = "printf beta > notes.txt && cat notes.txt";

// The shell tool's name in each agent's tool list (Claude Code's Bash, Codex's exec_command, Hermes's terminal,
// Grok's run_terminal_command, Antigravity's run_command, ...).
const SHELL = /^(bash|shell|shell_command|exec_command|run_shell_command|run_terminal_cmd|run_terminal_command|terminal|execute|exec|run_command)$/i;

const textOf = (content) => (typeof content === "string" ? content : (content ?? []).map((part) => part.text ?? "").join(""));

/** What the model answers: the shell call, then "done: <its output>" once the result is back; "ok" to anything else (titles, summaries). */
export function respond(request) {
  const last = request.messages.findLastIndex((m) => m.role === "user");
  if (!textOf(request.messages[last]?.content).includes(MARK)) return { content: "ok" };
  const turn = request.messages.slice(last + 1);
  // Antigravity sends its tool results in a "model" turn, which aimock reads as an empty assistant message.
  const result = turn.find((m) => m.role === "tool") ?? turn.find((m, i) => i > 0 && turn[i - 1].tool_calls && m.role === "assistant" && !m.tool_calls && !m.content);
  if (result) return { content: `done: ${textOf(result.content).trim().slice(0, 80)}` };
  const tool = (request.tools ?? []).find((t) => SHELL.test(t.function.name));
  if (!tool) return { content: `no shell tool among ${(request.tools ?? []).map((t) => t.function.name).join(", ")}` };
  const args = shellArguments(tool.function.parameters, request);
  return { toolCalls: [{ name: tool.function.name, arguments: JSON.stringify(args), ...(tool.namespace ? { namespace: tool.namespace } : {}) }] };
}

/** The command in whatever argument the tool's schema names, asking to leave the sandbox where the tool can. */
function shellArguments(schema, request) {
  const props = schema?.properties ?? {};
  const key = ["command", "cmd", "commands", "CommandLine"].find((k) => k in props) ?? Object.keys(props)[0];
  const args = { [key]: props[key]?.type === "array" ? ["bash", "-lc", COMMAND] : COMMAND };
  if ("description" in props) args.description = "Write beta into notes.txt";
  // Codex and DeepSeek Harness run commands in a sandbox where writing asks nothing: leaving it (Codex's
  // require_escalated, DeepSeek Harness's widest sandbox, its enum's last value) is what asks the user.
  if ("sandbox_permissions" in props) {
    const levels = props.sandbox_permissions.enum ?? ["require_escalated"];
    Object.assign(args, { sandbox_permissions: levels.includes("require_escalated") ? "require_escalated" : levels.at(-1), justification: "Write beta into notes.txt" });
  }
  // Antigravity's run_command needs its working directory: the workspace its system prompt lists that isn't the bot's memory.
  if ("Cwd" in props) {
    const listed = textOf(request.messages.find((m) => m.role === "system")?.content).match(/Active Workspaces:\n((?:- .*\n)+)/u)?.[1] ?? "";
    const cwd = listed.split("\n").map((line) => line.slice(2)).find((dir) => dir && !dir.endsWith("/bot-memory"));
    Object.assign(args, { Cwd: cwd, WaitMsBeforeAsync: 10000, toolSummary: "Notes rewrite", toolAction: "Writing notes" });
  }
  return args;
}
