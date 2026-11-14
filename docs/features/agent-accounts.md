# Agent accounts

An agent can have more than one login on a computer, and each bot picks which one it runs on, so two bots on the
same agent can work at once on different accounts ([#167](https://github.com/leepokai/Codync/issues/167)). Claude Code
and Codex have it. A bot that picks none uses the login the computer already has, which stays where it is.

"Agent account" means a login to an agent's provider. It isn't a Codync account (`AccountStore`, Clerk): keep the two
apart in code and copy.

Code: `host/src/agent/accounts.rs` (accounts, their environment, sign-in check, removal), `accounts/slot.rs` (what a
slot shares), `accounts/identity.rs` (who a slot signs in). Clients: an **Other accounts** section in the agent's
setup sheet and an **Account** choice under Agent in a bot's settings.

## One account is one folder

Each account has a folder (its slot) at `<data dir>/agent-accounts/<agent>/<id>`, and the agent's own variable points
a process at it. Two processes with different variables are two logins at once. This is the way Claude Code's own
documentation gives for several accounts.

| Agent | Points at the slot | Where the login lives | Checked |
|---|---|---|---|
| Claude Code | `CLAUDE_CONFIG_DIR` | macOS: a Keychain entry named after the folder's path, so each slot has its own. Linux, Windows: `.credentials.json` in the slot | A new slot reads as signed out beside a signed-in computer login (macOS, 2.1.323) |
| Codex | `CODEX_HOME` | `auth.json` in the slot | The same (macOS, 0.167.0) |

Rules that hold for every agent:

- **The agent's own CLI signs in and out.** Codync never reads or stores a token, and never asks the provider who an
  account is: the name shown comes from the slot's own files (`.claude.json`, the ID token in `auth.json`).
- **One account is one slot.** Bots on the same account share it. A second copy of a login would sign the first one
  out when its token is renewed.
- **`HOME` stays.** Replacing it would hide the Keychain and the logins of `git`, `ssh` and `gh`.
- **The host builds every path** from an id it generated and looks up in its store. A client's string never becomes
  a path, and an id that isn't one folder name is refused.

## What a slot shares

A slot separates more than the login: skills, rules, MCP servers, settings and sessions live in the same folder, and
an agent started on an empty one would lose them without a word. Before each start the host brings the slot up to
date with the user's own setup:

- **Folders are linked** (a junction on Windows, which needs no privilege), so both sides see one copy. That includes
  the folders sessions are stored in: a bot moved to another account picks its conversation up. A session folder the
  user doesn't have yet (Claude Code's, Codex's `sessions`) is made first, so the slot never grows one of its own
  that would go with the account.
- **Files the agent rewrites are copied.** A rewrite by rename replaces a link with a plain file.
- **Claude Code** shares only what is named (`skills`, `agents`, `commands`, `plugins`, `rules`, `output-styles`, the
  session folders, `CLAUDE.md`, `keybindings.json`), since it keeps runtime state beside its setup. `settings.json`
  is copied without the settings that choose a login (`apiKeyHelper`, `forceLoginMethod`, the token variables in
  `env`). The user's own MCP servers are one key of `~/.claude.json`, a file that also says who is signed in, so only
  that key is carried over.
- **Codex** links everything in `~/.codex` except the files that hold the login (`auth.json`, its locks,
  `models_cache.json`).

Known limits: a setup folder the slot already filled stays its own, and a Codex file rewritten by rename
(`config.toml`) is the slot's own copy from then on. In Codex's keyring mode the sign-ins of MCP servers are shared
by every slot (its keyring names them by server, not by home).

## Bots, limits and removal

- `account` on a bot is the id, or absent for the computer's own login. A bot moved to another agent, or given a
  custom command, leaves its account behind. A bot whose account is gone doesn't start: it never runs on another
  login in silence.
- Changing a bot's account restarts its agent at the next idle point, like a model change.
- A bot on a fallback runs on that agent's own login: an account belongs to one agent.
- **An account's saved keys stay out.** A process on an account starts without the login keys saved for the agent
  (Claude Code: `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `CLAUDE_CODE_OAUTH_TOKEN`; Codex: `OPENAI_API_KEY`,
  `CODEX_API_KEY`), the address saved with one, and a cloud computer's proxied ones: the agent prefers a key, and
  would run on it while the bot shows the account (`auth::launch_env`).
- **Each login is told apart everywhere it has state**: signed in or out, the refusal that outranks a check that still
  says signed in, the sign-in card (its `connectionRequest` carries `account`, and the apps open that account's
  sign-in), and the [fallback rest](agent-fallback.md) (`rest.<agent>#account-<id>`). A usage
  limit or sign-out on an account never rests or signs out the computer's own login.
- The Usage screens show the computer's own login. What an agent on an account reports stays out of them (the status
  line hook sees `CODYNC_AGENT_ACCOUNT` and forwards nothing).
- Removing an account is refused while a bot uses it, while an agent process still runs on it (a bot moved off it
  mid-turn restarts only when the turn ends) and while its sign-in terminal is open. Otherwise its CLI signs it out
  and the slot is deleted.

API: `addAgentAccount`, `removeAgentAccount`, `agentSetup` and `agentAuth` with `account`; each agent's `accounts`
in `hello.backends`. All additive, so no `minApp` change.

## Not built

- **Moving to another account when one runs out** ([#285](https://github.com/leepokai/Codync/issues/285)). A
  fallback step is identified by its agent alone (`fallback::tidy`, `step_of`, `park::left_key`), so two steps on
  one agent need that key to become agent and account. It is also account rotation, which providers' terms may not
  allow: the owner's call.
- **Limits per account** in the Usage screens. They would come from the same local sources per slot, never from a
  provider API.
- **Models per account.** The model list is read on the computer's own login.
- **Google Antigravity and Gemini CLI.** Antigravity's separation is unsettled: a run on its ACP server 1.4.3 kept
  two homes apart with `GEMINI_HOME` and `AGY_ACP_FORCE_FILE_STORAGE=1`, but its CLI is reported to ignore
  `GEMINI_HOME` and keep one login in the Keychain. It needs a run with two real accounts first
  ([PR #105](https://github.com/leepokai/Codync/pull/105) covers it with `GEMINI_HOME` alone). Gemini CLI
  (`GEMINI_CLI_HOME`, login in `.gemini/oauth_creds.json`) is read from its source only, never run.
- **Naming an account**, and adopting a folder the user already has (`~/.claude-work`) without a new sign-in.

## Why not another way

- **Swapping the computer's one login.** Only one account at a time, and it races the agents already running.
- **A long-lived token in the environment.** Codync would have to keep the token, and the agent can do less with it.
- **A proxy or account pool.** It calls the provider with the agent's credentials, which the
  [usage rule](../../CLAUDE.md#architecture) forbids and providers' terms name.
- **Each account as its own agent id.** Many places compare agent ids as strings and would break in silence.
