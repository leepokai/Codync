# Extensions and agent setup

The Extensions screen configures capabilities installed on one computer. Account membership does not install the same capability on every computer, so each computer has its own Extensions, opened from that computer and never switched inside: with several computers, from the Extensions button on its heading in the bot list (desktop and iPhone); on the desktop with one computer, from the sidebar footer; on the iPhone, also from its row in Manage computers.

Under the open section, one line says where that section comes from: agents from the ACP registry, connectors from the MCP Registry, skills from Anthropic, and that they install on this computer. Credentials sits on the title row, beside the computer name, on the iPhone, Android and desktop, so the list can use the rest of the screen. The terminal has no Credentials control on this sheet. The desktop and iPhone installed count is labeled **Connectors & skills**, and opens a library with that same title. It includes connected apps (stored as connectors), but excludes agents; agents show their own installation status in the Agents section. The terminal already separates these categories and does not show a combined installed count.

| Capability | Implementation | Behavior |
| --- | --- | --- |
| Agents | `host/src/agent/backends.rs`, `host/src/agent/registry.rs`, `host/src/agent/node.rs` | Detect installed harnesses, resolve ACP registry adapters, provide Node.js for npm agents |
| Connectors | `host/src/market/mod.rs`, `registry.rs`, `install.rs` | Discover MCP servers or add one manually; install per computer; on for every bot unless a bot turns it off |
| Connected apps | `host/src/market/composio.rs` | Expose connected Composio apps as connectors |
| Skills | `host/src/market/skills.rs` | Instruction folders containing `SKILL.md`, stored under `~/.codync/skills/<id>`; on for every bot unless a bot turns it off |

Composio Connect is featured in Connectors through the official MCP Registry
(`io.github.ComposioHQ/composio`, `https://connect.composio.dev/mcp`). Install it
and sign in with the shared MCP OAuth flow; no Platform API key is needed.
The agent discovers and connects apps through Composio's hosted tools. For
manual header authentication, add the endpoint as a custom connector with
`x-consumer-api-key: ck_…`. Consumer keys are not interchangeable with Platform
project keys (`ak_…`, sent as `x-api-key`), which the legacy Platform integration uses. The Apps shelf and Platform-key
setup have been removed from Extensions; existing connections and pending
connection requests remain supported.
All clients render the host's connector catalog and use its existing sign-in
flow; Composio is installed from Connectors on every client. The terminal already
has only Agents, Connectors and Skills tabs.

Install runs the CLI's own native installer where it has one, on Windows too (Claude Code, Codex, Grok, Qwen, Droid, Qoder, Cursor…, `Install` in `backends.rs`); CLIs published only on npm (Gemini, Cline, Auggie, Kilo…) install with `codync-host npm install -g <package>`, npm on the computer's own Node or else on Codync's. A CLI with its own ACP mode then runs as installed, without Node.js. Refreshing the agents re-reads the PATH a fresh login would get (the login shell's; on Windows the user's and machine's PATH from the registry), so a CLI found by an installer that just ran shows up without restarting the host. Many agents' ACP adapters run through `npx` (Claude, Codex, Gemini, Qwen…). On a computer without Node.js, the agent list marks each of them `node: "missing"` (only then is the field there; `added` once Codync's copy is in place) and they aren't available; Extensions shows one *Add Node.js* notice above the agents, each such agent reads *Needs Node.js*, and its setup sheet gets a Node.js step between Install and Sign in. Adding it (`agentSetup {step: "node"}`, a setup terminal running `codync-host node`) downloads the official Node.js of a pinned version (`VERSION` and SHA-256s in `node.rs`) once into `~/.codync/node/<version>`; npm installs with it go to `~/.codync/node/global`. Both folders come last on the agent search path, so the computer's own Node and CLIs always win, and nothing outside `~/.codync` changes. Linux builds of Node need glibc: on musl distros (Alpine) install Node.js yourself.

Extensions shows one section at a time — Agents, Connectors, or Skills — and each is one list that scrolls; there are no pages. The lists are lazy (iPhone `LazyVGrid`, Android `LazyColumn`), so only the rows near view are built. Agents and Skills arrive whole. The Connectors section opens with the first registry page (`marketConnectors {search, cursor}` → `{items, nextCursor}`, 60 metadata records per API page); while a cursor remains, placeholder rows end the list (`LoadMoreRow`), and when they come into view the client asks for the page at the cursor, one page per ask, then again each time the cursor moves on while the end stays in view (`ConnectorCatalog.loadMore`). A page with nothing Codync can install still moves the cursor, so the list reads on and never stalls on an empty answer. If a fetch fails, the rows stay, the error shows under the list, the end of the list becomes a Try again button, and it also asks again once it scrolls back into view. A registry that answers with the cursor it was asked for ends the list there: the host drops that cursor rather than serve the same page forever. Searching starts the list over; late responses from an older search are ignored. In the terminal, moving the cursor within about a screen of the end of Connectors asks for the next page the same way. Listed: stdio npm/PyPI/OCI/NuGet packages (the registry's runtime and package arguments become inputs) and streamable-HTTP or SSE remotes (URL `{variables}` become inputs). Not listed: MCPB bundles and packages that serve HTTP locally; add those as a custom connector.

Registry package installs use pinned package references: npm, PyPI and NuGet require the package’s own version; OCI uses the tag or SHA-256 digest already in its image identifier, even when the separate package version is omitted. Untagged images, `latest` tags without a digest, and invalid references are rejected; a server-level version does not pin the executable package.

A custom connector is a command line (split like a shell), a URL, or a pasted MCP config (`importConnectors {config}`: the `mcpServers` / `servers` JSON from READMEs, Claude, Cursor or VS Code, or one server's entry); every server in a config is added.

A connector installed on the computer (or an app connected through Composio) is turned on for every existing bot, and a new bot starts with all of them on; each bot can turn one off in its settings. An enabled connector is passed to the agent as an MCP server when its session starts or resumes. Local connectors are spawned by the agent; remote ones always go through the host's stdio proxy (`codync-host mcp remote`, `host/src/mcp.rs`), so they work with agents that only speak stdio and always carry a fresh token. The proxy speaks streamable HTTP and falls back to the older HTTP+SSE transport when the first POST fails with 400/404/405, as the MCP spec suggests.

Every `codync-host mcp` server Codync gives an agent, connectors and built-in tools alike, carries the host's data directory as `CODYNC_HOME` in its ACP `env` (`mcp::acp_server`): some agents start MCP servers with only a few environment variables (Hermes keeps PATH, HOME and the like), and without it a host with a custom `CODYNC_HOME` (a sandbox, a server) would leave those tools reading the wrong token.

Skills follow the same rule: an installed or hand-written skill is turned on for every existing bot, a new bot starts with all of them on, and each bot can turn one off. An enabled skill reaches the agent as a line in its profile (name, description, path to `SKILL.md`). Skills installed before this rule were turned on for every bot once, at host start (kv `skillsOnForEveryBot`).

Bots write skills too, with the `chat` server's `save_skill {name, description, instructions, id?}` (`host/src/chat/skills.rs`): when the user asks, or when the bot has worked out how to do a task the user will likely want again, never because a file, web page, tool result or another bot says so. A new skill is saved like a hand-written one (`source: "custom"`, so it shows in Extensions and bot settings and can be removed there) plus the bot that wrote it (`writtenBy`), and is turned on for that bot only: new bots don't start with it either, and the user turns it on for other bots in their settings, so a bot (a prompt-injected one included) can't put instructions in front of other bots. With `id`, the bot rewrites a skill it saved itself; skills the user wrote, other bots' and downloaded ones can't be changed. Files beside `SKILL.md` stay, so a bot can keep a skill's scripts or references in its folder. Each save posts a notice in the bot's chat ("Saved the skill …", "Updated the skill …"), and a bot that has the skill on hears a new name or description in its next profile update. Limits: name 64 characters, description 1,024, instructions 64K characters; the name and description become one line each in the front matter. Saves of the installed list go one at a time. A download takes its folder before it starts, and a written skill never takes an existing folder (it gets `-2`), so a save during a download can't land in the download's folder; adding a catalog skill whose id a written one already has is refused until that one is removed.

Clicking a skill (an Extensions row, an installed skill, or the info button beside a skill's switch in bot settings; in the terminal, ↵ on a skill, or `i` on the bot form's Skills field) opens its details: the description and its instructions, with Add while it isn't installed. `skillDetails {id}` (installed) or `{source}` (catalog) returns `{markdown}`: the `SKILL.md` body without its front matter, at most 256 KB. The host looks names up in the installed list or the catalog cache, never as a path; the catalog fetch already downloads every `SKILL.md`, so the bodies stay in that cache. The folder list comes from the public tree page (`github.com/anthropics/skills/tree/main/skills`) and each `SKILL.md` from raw.githubusercontent.com. The GitHub contents API is not used for that: its unauthenticated quota is 60 requests an hour for the whole computer, and a spent quota answers 403. An Extensions row opens its details; only its Add button adds. A connector row opens the install sheet without installing (`addsAtOnce` off), while the row's Add opens it ready to install at once when there is nothing to fill in. Opening an agent's sheet checks its sign-in only once the agent is set up on the computer (its CLI is installed, or an earlier check ran it): a registry agent never checked shows **Set up** instead, since its first start downloads it. The terminal does the same: ↵ on a registry connector opens its Add connector sheet (with "No setup needed." when there is nothing to fill in), and ↵ on that sheet adds it; an agent's sheet opened there starts no agent that isn't set up, and its last row reads Set up instead of Check again.

Registry-backed package connectors follow their upstream release. The host checks the Registry periodically after credentials are unlocked and checks again immediately before a package process starts. If a newer pinned npm, PyPI, OCI or NuGet reference is available, only that package/image argument is replaced; the running process is not interrupted, and the new version applies on its next launch. Network failures, missing packages and attempted downgrades leave the installed reference unchanged. Custom command connectors are never updated automatically.

## Codex usage

The host asks the installed Codex CLI's app-server `account/rateLimits/read` method for the current account's plan windows every five minutes. Executable lookup and process startup run on a blocking worker. It initializes the CLI transport without creating a thread or model turn, then stops the probe through the host's existing process-tree cleanup. Startup, handshake and account-read failures retain their phase and cause in structured debug logs. The canonical `rateLimitsByLimitId.codex` bucket takes precedence over the older single `rateLimits` view. Reset timestamps are Unix seconds and become milliseconds in the existing `usage` event.

The CLI must be signed in as the user running the host. Missing, rejected or unavailable account readings remove the Codex provider rather than reusing session-log snapshots. Windows whose reported reset has already passed are omitted. A signed-out host therefore does not keep showing an old percentage with “resets in 0m.” Sign in with `codex login --device-auth`, or use Extensions > Codex > Sign in. Existing client usage views render the same window shape without a protocol change.

## WorkBuddy

WorkBuddy is a built-in agent that runs the CLI bundled inside the WorkBuddy desktop app (`WorkBuddy AI.app` from workbuddy.ai or `WorkBuddy.app` from codebuddy.cn on macOS; `%LOCALAPPDATA%\Programs\WorkBuddy AI` or `…\WorkBuddy` on Windows). The app ships only the lite bundle (`codebuddy-lite-wb.mjs`), so Codync sets `CODEBUDDY_FORCE_LITE_WB_BUNDLE=1` and starts `{bin} --serve --acp`. `--acp` alone loads a headless bundle the app does not include, and the process exits. `--serve` is what starts ACP; `--acp` selects its stdio transport (the HTTP banner on stdout is ignored). That file is also named `codebuddy`, so Codync does not take it from `PATH`: a `codebuddy` command there stays [CodeBuddy Code](https://www.npmjs.com/package/@tencent-ai/codebuddy-code). It is a Node script, so the agent list asks for Node.js (Codync's own when the computer has none). There is no installer in Codync, and no Linux build, so Linux doesn't list it. The app's login is encrypted for the app, so the CLI does not see it: a prompt with no token comes back as `stopReason: refusal` with the error in `_meta["codebuddy.ai/errorMessage"]`, which Codync reads as that error (`acp::codebuddy::prompt_result`): a sign-in card holding the message, or the bot's next agent. Its session opens signed out, so a check asks `_codebuddy.ai/getUserInfo` instead (only whether it names an account: the answer carries tokens). On `authenticate`, Codync opens the http(s) page from the `_codebuddy.ai/authUrl` notification. Select **WorkBuddy** and sign in from that card. It honors `CODEBUDDY_DISABLE_AUTO_MEMORY`, so a bot can keep WorkBuddy's own memory on the same way as CodeBuddy Code.

## Hermes Agent

Hermes Agent is a built-in agent that runs its installed CLI with `hermes acp`, without an ACP registry adapter. Install it using the [official installation guide](https://hermes-agent.nousresearch.com/docs/getting-started/installation), then run `hermes acp --setup` to choose a provider and model. In Codync, refresh the agents and select **Hermes Agent** when creating a bot; leave the custom command empty.

The Unix installer puts its launcher at `~/.local/bin/hermes`, which Codync already searches. On Windows, Codync also searches `%LOCALAPPDATA%\hermes\bin` (the current installer) and `%LOCALAPPDATA%\hermes\hermes-agent\.hermes\bin` (older source installs), so Hermes can be detected without adding it to PATH. For a custom install location, add its launcher directory to PATH before refreshing agents.

Hermes keeps its own memory, on by default: `MEMORY.md` and `USER.md` in `~/.hermes/memories`, loaded by every Hermes session (other Hermes bots and the `hermes` CLI too). Codync can't turn it off per bot, its memory screen doesn't show it, and forgetting something in Codync doesn't reach it. For one memory, set `memory.memory_enabled` and `memory.user_profile_enabled` to `false` in `~/.hermes/config.yaml`; that applies to every Hermes session.

## Commands

Typing `/` at the start of a message to a bot (desktop, iPhone, Android, terminal) lists that bot's commands, filtered by what follows the `/` (a command's name or title): the built-in ones first, then the skills the bot has on. A group chat lists its members' commands the same way: Fast mode for every member that offers it (on once all of them are; picking it turns each member's own Fast mode on or back off, so it also applies in their own chats), then every skill one of them has on. A `/skill …` message in a group reaches each member that has the skill on as "read this skill first"; the others read it as typed. Each row shows an icon, the title and what it does. The list floats above the box on the desktop (↑/↓ move the highlight, Tab or Return picks it, Escape hides the list until the draft changes, hover and click work too), sits inline above the box on the iPhone (a tap picks; it isn't a menu, so the system-menu rule doesn't apply), and is the highlighted list over the transcript in the terminal (↑/↓, Tab or ↵).

| Command | Title | Does |
| --- | --- | --- |
| `/fast` | Fast mode | Switches [Fast mode](#fast-mode) with `updateBot`; listed only while the bot offers it. Its detail says what picking it does, or why the agent refused it (warning color). |
| `/<skill id>` | the skill's name | Fills in `/<skill id> ` for the rest of the message. |

Picking a built-in command, or sending a message that is just one (`/fast`), runs it in the client and clears the box; nothing reaches the agent. Return on a skill whose name is already typed out sends the message. The host sees a message starting with `/<id of a skill the bot has on>` and hands the agent `[Codync: the user picked the "<name>" skill for this message. Read <path> first and follow it.]` followed by the rest of the message; the chat keeps the text as typed. Any other `/command`, including `/fast` for a bot without Fast mode on offer, reaches the agent unchanged, so its own commands keep working.

## Sign-in for remote connectors (`host/src/market/oauth.rs`)

Installing a remote connector probes it: a 401 to `initialize`, or (with no headers given) protected-resource metadata naming an authorization server, for servers like Call2Me that only refuse tool calls, marks it `signedOut`, and bots don't get it until someone signs in. Sign-in follows the MCP authorization spec: protected-resource metadata → authorization-server metadata → dynamic client registration (both redirect URIs at once) → PKCE. Tokens stay on the computer and are refreshed on use (or after a 401).

| Where the user signs in | Redirect URI | Who finishes |
| --- | --- | --- |
| On the computer (desktop app, TUI; loopback caller) | `http://127.0.0.1:<port>/oauth/callback` | the host's callback page |
| On the phone (E2E channel caller) | `<cloud>/v1/oauth/callback` → 302 `codync://oauth?…` | the app's system sign-in sheet sends `connectorSignInFinish` |

The cloud page is stateless and the code is useless without the PKCE verifier held by the host. Servers without dynamic client registration (e.g. GitHub) need a token header instead (custom connector, URL + headers). `connectorTarget` (loopback only) hands the proxy the URL and headers.

Registry packages install only at the exact version the MCP Registry lists (no `latest`), since they run as the user. An enabled skill tells the bot where its instruction folder lives. These are distinct from the built-in `team` and `computer` MCP services.

Registry and repository metadata are untrusted: the host validates names and paths before installing. Secret values stay on the computer; listings report whether required values are configured. Adapter downloads and external authorization depend on the selected provider.

To check a change, use a disposable bot on the intended computer, verify discovery/install/configuration, start a new session as needed, and confirm the agent actually receives the capability. Listing an item alone does not verify its authentication or runtime behavior.

See [file structure](../architecture/file-structure.md), [bot collaboration](bot-collaboration.md), and [remote screen](remote-screen.md).

## Model selection

The shared bot editor requests `agentModels {backend}` from the selected computer. The host starts an isolated ACP session under its probe directory with saved agent credentials, no MCP servers and no prompt, then closes the process. The response is `{models: [{id, name, description, options, optionsError?}], currentModelId, fastMode, defaultModelOptions}`.

Discovery prefers ACP `configOptions` with category `model` (including grouped options), with conventional `model`/`models` IDs as a compatibility fallback. Older agents may expose `models.availableModels`. Model names and IDs come from the agent; Codync does not bundle a provider catalog. See the [ACP config option contract](https://agentclientprotocol.com/protocol/v1/session-config-options).

For non-Cursor agents, discovery reads reasoning selects from ACP `thought_level` options (with legacy effort IDs as a fallback) and probes each model in the disposable session. A model that refuses selection keeps its catalog entry with `optionsError`; other models remain usable. Grouped effort values are flattened. The inherited/default choice has a single reset control. `defaultModelOptions: true` advertises that this host applies options even when no explicit model is chosen; clients hide Default-model options on older hosts without this capability.

A model's `options` are its own settings, such as reasoning effort or fast mode: `[{id, name, values: [{id, name}], default}]`. Every `initialize` sends `clientCapabilities._meta.parameterizedModelPicker: true`; Cursor then lists plain models and, on `cursor/list_available_models` (asked only of the Cursor backend), each model's settings as ACP select config options. Without the flag Cursor offers one fixed variant per model (`grok-4.7[reasoning_effort=high,fast=true]`) and rejects every other. The bot stores the picks as `modelOptions` (option id → value; an absent id means the agent's default). After choosing the model with `session/set_config_option`, the host sets each option the same way, on new sessions and again after `session/load`, because Cursor resumes a session on its last-used model. A bot saved with a variant ID keeps working: when the agent lists the plain model instead, the bracket's `key=value` pairs become options (Auto's `default[]` becomes `default`). Some values change Cursor's billing: a larger context (Grok 4.7 at 500K) turns on its Max Mode. Changing the model or agent clears the options; the host also clears them when an app that predates them changes the model.

The graphical editors summarize harness, model, effort and Fast mode in one expandable row. Expanded controls keep harness and model accessible, with one shared search list and refresh at its right. Choosing a harness opens its model list; choosing a model hides only that list. Effort uses advertised levels and one Default reset label; Fast mode shares the panel. The terminal keeps keyboard fields and shows the effective default in its Options row (←→ moves, space changes). Errors are visible and retryable. Custom commands and agents without model discovery retain manual ID entry; an existing unlisted ID is preserved. The probe uses an isolated directory, so project-specific agent configuration can differ from the real bot session. Changing the model restarts the bot's agent sessions with fresh model context and preserves the stored chat history. The agent remains responsible for validating availability when a session starts.

## Fast mode

Claude Code's and Codex's ACP adapters offer Fast mode (faster replies for more usage) as a session config option: Claude's `fast`, Codex's `fast-mode`, each a `select` of `on`/`off` (a boolean for clients that opt into boolean options), listed only while the session's model supports it. Only such an `on`/`off` or boolean option counts: Cursor's `fast` (`true`/`false`) is one of a model's options (see Model selection), so Fast mode leaves it to the bot's `modelOptions`. `fastMode` in `agentModels` says whether the probe session offered it, so it reflects the agent's default model.

The host keeps each live session's Fast mode option as the agent last listed it: from `session/new` (after the model switch, whose answer can change the options), `session/load`, `session/fork`, `session/set_config_option` answers and `config_option_update` notifications. Before every turn it switches the session to the bot's `fastMode` with `session/set_config_option` when the value differs, so changing the setting needs no agent restart and keeps the context. The adapters start each session or process at their own default, which is why a resumed session is switched again.

The bot's runtime carries what the agent says (`fastModeSupport`, `fastModeIssue` in the bot payload):

| `fastModeSupport` | Meaning |
|---|---|
| `unknown` | No session has listed its options yet. A session that lists no config options at all settles it as `unsupported`. |
| `offered` | The agent offers it for the session's model. |
| `unsupported` | The session's model doesn't (the chat gets an info notice while the bot asks for it). |
| `refused` | The agent turned it off right after the bot asked, or despite the switch: Claude does this for an account without extra usage and appends the reason to the option's description, which becomes `fastModeIssue`. The host stops asking until the user changes the setting or the host restarts. |

`offered` and `unsupported` are kept across host restarts (`fast-mode.<bot>` in the kv store); a refusal isn't, so a new host asks again.

Clients show Fast mode for an agent bot unless its runtime says `unsupported` (an `unknown` bot may offer it; its first session settles it), and always while it's on (so it can be turned off). A group has no Fast mode of its own: its `/fast` sets its members' (see [commands](#commands)). It shows in:

- Composer: the `/fast` [command](#commands), not a button. The terminal UI also shows `⚡ fast` or `⚡ refused` in the chat header.
- Bot settings: a Fast mode switch row under Model when the agent's model list offers it (never for a custom command), when the bot's own sessions do, or while it's on. While refused, its detail gives the reason. The terminal UI form always lists the field and notes when the agent doesn't offer it.

## Secure connection setup

See [Connector setup and credentials](connector-credentials.md) for the shared
macOS/Linux vault, optional 1Password references, in-chat connection requests and
MCP readiness checks. Single-option connectors without setup fields start adding
immediately; setup failures remain visible for retry.
