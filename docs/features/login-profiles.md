# Login profiles

An agent can have more than one login on a computer, and each bot picks which one it runs on, so two bots on the
same agent can work at once on different logins ([#167](https://github.com/leepokai/Codync/issues/167)), and a
fallback step can be another login of the same agent ([#285](https://github.com/leepokai/Codync/issues/285)). A bot
that picks none uses the **default login profile**: the login the computer already has, which stays where it is.

A login profile is one of two kinds:

- **Account**: the agent's own sign-in, in a folder of the profile's. Claude Code, Codex, OpenCode, Pi, Google
  Antigravity, Devin and Gemini CLI have it.
- **API key**: environment variables the user types, or imports from their login shell, which Codync keeps in its
  vault and sets for the agent's process. Every agent can have one.

The name is always "login profile", never "profile": a bot has a profile of its own. It also isn't a Codync account
(`AccountStore`, Clerk): keep the two apart in code and copy.

Code: `host/src/agent/login_profiles.rs` is Codync's side: the records, both kinds, the sign-in check, names,
removal and the processes that run on a profile; `login_profiles/account.rs` holds an account profile's folder and
sign-in, `login_profiles/keys.rs` an API key profile's variables and the import from the login shell. The `codync-accounts` crate (`host/crates/accounts`) holds what is true of the
agents and no app, for the account kind: `agents.rs` (how each agent keeps a login in a folder), `slot.rs` (what a
slot shares), `identity.rs` (who a login is) and the `Account` record. Clients: one **Login profiles** section in
the agent's setup sheet (the default one first, then the others; each can be named), a **Login profile** choice
under Agent in a bot's settings, and the same choice on a fallback. The section shows for every agent, installed
or not: any agent takes an API key, while **Add account** needs the agent's own CLI on the computer.

## An account is one folder

Each account profile has a folder (its slot) at `<data dir>/login-profiles/<agent>/<id>`, and the agent's own variable points
a process at it. Two processes with different variables are two logins at once. This is the way Claude Code's own
documentation gives for several accounts.

| Agent | Points at the slot | Signs in | Where the login lives | Checked |
|---|---|---|---|---|
| Claude Code | `CLAUDE_CONFIG_DIR` | its CLI, in a terminal | macOS: a Keychain entry named after the folder's path, so each slot has its own. Linux, Windows: `.credentials.json` in the slot | A second account signed in and ran beside the computer's own (macOS, 2.1.323) |
| Codex | `CODEX_HOME` | its CLI | `auth.json` in the slot | A new slot reads as signed out beside a signed-in computer login (macOS, 0.167.0) |
| OpenCode | `XDG_DATA_HOME`, `XDG_STATE_HOME`; `OPENCODE_AUTH_CONTENT` cleared | its CLI | `opencode/auth.json` in the data folder (its database in 2.x) | The same (macOS, 1.19.5) |
| Pi | `PI_CODING_AGENT_DIR` | its CLI (`/login`) | `auth.json` in the slot | The slot lists only what its own logins offer (macOS) |
| Google Antigravity | `GEMINI_HOME`, `AGY_ACP_FORCE_FILE_STORAGE=1` | its ACP server (`authenticate`: a browser on the computer) | `antigravity-acp/acp_token.json` in the slot; without the second variable, the Keychain | The server answers "Authentication required" on a new slot beside a signed-in own login (macOS, server 1.5.0); no second Google account signed in yet |
| Devin | `XDG_DATA_HOME`, `XDG_STATE_HOME` | its CLI | `devin/credentials.toml` in the data folder | Not run: the CLI isn't installed here |
| Gemini CLI | `GEMINI_CLI_HOME`; `GEMINI_FORCE_ENCRYPTED_FILE_STORAGE=false` | its CLI | `.gemini/oauth_creds.json` under the slot | Read from its source (0.66.0), not run |

Agents left out, and why: Cursor keeps its login in one Keychain entry with no variable to move it; Grok has no
variable for its home; GitHub Copilot CLI has `COPILOT_HOME`, but where its login goes couldn't be seen on a computer
that isn't signed in to it; oh-my-pi keeps its logins in a database; Hermes builds a whole runtime per home.

Rules that hold for every agent:

- **The agent signs in and out itself**, through its CLI or its ACP server. For an account, Codync never stores a
  token, never uses one, and never asks the provider who it is: the name shown comes from the login's own files
  (`.claude.json`, the ID token in `auth.json`, the claims of a saved ChatGPT token, `google_accounts.json`), or is
  "Login profile 2" where they don't say. The user can name any login profile, the default one too
  (`renameLoginProfile`).
- **One account is one slot.** Bots on the same account share it. A second copy of a login would sign the first one
  out when its token is renewed.
- **`HOME` stays.** Replacing it would hide the Keychain and the logins of `git`, `ssh` and `gh`.
- **The host builds every path** from an id it generated and looks up in its store. A client's string never becomes
  a path, and an id that isn't one folder name is refused.

## An API key is variables in the vault

An API key profile holds named environment variables (a key, and often the address that goes with it), for an
agent that has no accounts or for a key the user would otherwise export in their shell: a custom provider in
OpenCode's config that reads `"apiKey": "{env:MY_KEY}"`, a gateway's `ANTHROPIC_API_KEY` and `ANTHROPIC_BASE_URL`.

- **Kept in the vault** (`market::vault`, slot `login-profile:<id>`), the same encrypted store as an agent's saved
  keys, through the same code (`auth::save_env`, `env_without`). Values never reach a client, a log, analytics or a
  setup terminal (`agentSetup` and `agentAuthenticate` refuse an API key profile); clients see the names (`vars`).
- **On a Codync Cloud computer, provider keys go to the keeper**, as an agent's saved keys do
  ([`auth::proxied`](../../host/src/agent/auth/proxied.rs), slots `login-profile:<id>:<VAR>`): the agent starts with
  a placeholder and a grant, and can call the provider but never read the key. A key saved with its own base-URL
  variable, and every other variable, stays readable by the agent.
- **Typed or imported.** The key form prefills the names an agent's key usually goes in (`loginKeys`). **Import
  from login shell** lists the names the user's login shell sets (`loginShellVariables`: `$SHELL -ilc`, read into a
  file only the user can open; on Windows the host's own environment), without the shell's and the computer's own
  (`PATH`, `HOME`, `CODYNC_*`…); the user ticks some, and the host reads those values itself and stores them, along
  with what the form holds (one `setLoginProfileKeys` with `vars` and `import`). It is a copy taken then: nothing reads the login shell when an agent starts, and no variable reaches an agent the user
  didn't put in a profile.
- **Set for the process**, over a same-named saved key; the agent's saved login keys are dropped as for an account.
- **Counted as signed in once it has a key.** A key can't be tried without calling the provider with it. A turn
  the agent refuses for auth marks the profile signed out, and its sign-in card opens the key form. A card for a
  profile the app hasn't heard of yet (added from another app) opens no sign-in: its kind isn't known.
- **No usage to read**, and nothing to share: the agent runs in its own home, with its own setup and sessions.

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
  `models_cache.json`). **Pi** does the same with `~/.pi/agent` (but `auth.json`), and **Gemini CLI** with
  `~/.gemini` into the slot's `.gemini`.
- **Google Antigravity** links everything in `~/.gemini` except its ACP server's folder, where only the
  conversations (`conversations`, `brain`) are shared: the sign-in and the choice of sign-in stay the slot's.
- **OpenCode and Devin** need nothing: their setup is in `~/.config`, which every account reads. Their sessions are
  in the data folder, so a bot moved to another account starts a new one. The variables are XDG's own, so the
  programs the agent runs see them too.

Known limits: a setup folder the slot already filled stays its own, and a Codex file rewritten by rename
(`config.toml`) is the slot's own copy from then on. In Codex's keyring mode the sign-ins of MCP servers are shared
by every slot (its keyring names them by server, not by home).

## Bots, limits and removal

- `loginProfile` on a bot is the id, or absent for the default one. A bot moved to another agent, or given a
  custom command, leaves its login profile behind. A bot whose account is gone doesn't start: it never runs on another
  login in silence.
- Changing a bot's login profile restarts its agent at the next idle point, like a model change.
- **A fallback step names its own login profile** (`fallbacks[].loginProfile`), or none for the default one. A login profile belongs
  to one agent. The chain has each login once, so an agent comes again only on another of its login profiles: Claude Code
  → Claude Code on a second login profile → Codex. Steps, parked sessions and rests go by login
  (`fallback::login`: the agent, or `<agent>#profile-<id>`). Moving on when a login profile runs out is account
  rotation, which a provider's terms may not allow: it is the user's to set up, and nothing does it unasked.
- **A login profile's saved keys stay out.** A process on a login profile starts without the login keys saved for the agent
  (Claude Code: `ANTHROPIC_API_KEY`, `ANTHROPIC_AUTH_TOKEN`, `CLAUDE_CODE_OAUTH_TOKEN`; Codex: `OPENAI_API_KEY`,
  `CODEX_API_KEY`), the address saved with one, and a cloud computer's proxied ones: the agent prefers a key, and
  would run on it while the bot shows the login profile (`auth::launch_env`).
- **Each login is told apart everywhere it has state**: signed in or out, the refusal that outranks a check that still
  says signed in, the sign-in card (its `connectionRequest` carries `loginProfile`, and the apps open that login profile's
  sign-in), and the [fallback rest](agent-fallback.md) (`rest.<agent>#profile-<id>`). A usage
  limit or sign-out on a login profile never rests or signs out the computer's own login.
- The Usage screens list each signed-in account under its agent, beside the default login, each with its own limits
  ([usage limits](usage-limits.md#login-profiles)). An API key has no plan to read.
- Removing a login profile is refused while a bot or one of its fallbacks uses it (saving that bot would fail once the
  account is gone), while an agent process still runs on it (a bot moved off it mid-turn restarts only when the
  turn ends), while its usage is being checked (up to a minute) and while its sign-in is open, in a terminal or in the agent's own browser flow. Otherwise an account's
  CLI signs it out and the slot is deleted; an API key profile's vault record is deleted, which needs no open vault.

API: `addLoginProfile` (with `kind`), `renameLoginProfile`, `removeLoginProfile`, `setLoginProfileKeys`,
`loginShellVariables`; `agentSetup` (a terminal sign-in), `agentAuthenticate` (the agent's own, for an agent whose
`loginProfileSignIn` is `browser`) and `agentAuth` with `loginProfile`; each agent's `loginProfiles`,
`defaultLoginProfile`, `loginProfileSignIn` and `loginKeys` in `hello.backends`; `loginProfile` on a bot, on each of
its fallbacks, on `agentStep`, on each `resting` entry and on a sign-in card. All additive, so no `minApp` change.

## Not built

- **Limits of an account** of an agent other than Claude Code and Codex: its row says **No reading yet**.
- **Models per login profile.** The model list is read on the computer's own login.
- **A second real account on each agent.** Only Claude Code has had two accounts signed in at once here. The
  others are checked as far as the table says.
- **Adopting a folder the user already has** (`~/.claude-work`) without a new sign-in.
- **Windows.** The code builds and lints for Windows (junctions for folders, a copy where a file link needs a
  privilege), but nothing has run there yet.

## Why not another way

- **Swapping the computer's one login.** Only one account at a time, and it races the agents already running.
- **A long-lived token in the environment.** Codync would have to keep the token, and the agent can do less with it.
- **A proxy or account pool.** It calls the provider with the agent's credentials, which the
  [usage rule](../../CLAUDE.md#architecture) forbids and providers' terms name.
- **Each account as its own agent id.** Many places compare agent ids as strings and would break in silence.
