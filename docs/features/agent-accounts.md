# Agent accounts

An agent can keep more than one sign-in on a computer: the computer's own
(default) account and any number of isolated profiles. The host owns the
profiles and the sign-in; clients only list accounts, ask to add one, remove
one, or sign the default one out. Provider tokens stay inside the profile
folder on the computer.

## Isolated profiles

`host/src/agent/account.rs` keeps a profile per account under the host's data
directory (`accounts/<id>/account.json`) and points the agent at it with the
harness's own home variable:

| Agent | Variable | Credential file checked after sign-in |
| --- | --- | --- |
| Claude Code | `CLAUDE_CONFIG_DIR` | `.credentials.json` |
| Codex | `CODEX_HOME` | `auth.json` |
| Grok Build | `GROK_HOME` | `auth.json` |
| Gemini CLI | `GEMINI_CLI_HOME` | `.gemini/oauth_creds.json` |
| Google Antigravity | `GEMINI_HOME` (+ `AGY_ACP_FORCE_FILE_STORAGE=1`) | `antigravity-acp/acp_token.json` |
| OpenCode | `XDG_DATA_HOME` (+ `XDG_CONFIG_HOME=<profile>/config`) | `opencode/auth.json` |

A profile process also drops ambient provider keys (`ANTHROPIC_API_KEY`,
`OPENAI_API_KEY`, `XAI_API_KEY`, the Google/Gemini keys) so it cannot borrow
the computer's own sign-in.

Adding an account starts the agent with that profile environment:

1. If the agent advertises a browser/agent ACP sign-in method, the host calls
   `authenticate` on it.
2. Otherwise the harness's CLI login runs with the profile environment.
3. Antigravity keeps its `oauth-personal` settings and ACP flow.
4. The host checks the credential file above; a profile that did not produce
   one is removed instead of being saved.

## Removing and signing out

- **Remove account** deletes an isolated profile. The host first runs the
  harness's logout command inside that profile (best effort), then deletes the
  folder. A profile a bot still uses is refused until the bot picks another.
- **Sign out default** affects only the computer's own sign-in: the host runs
  the harness's logout command with the normal launch environment, or removes
  its credential files when it has no logout command (Gemini and
  Antigravity). Isolated profiles are untouched.

`accounts` and `signOut` on each backend in `hello` tell clients which agents
support each action. The desktop agents panel and the bot account picker on
desktop, iPhone and Android read the same host state. The TUI keeps its
existing Antigravity account selection and does not add or remove accounts.

Host methods: `listAccounts`, `accountAuthenticate`, `deleteAccount` and
`agentSignOut` (returns the refreshed `auth` check).
