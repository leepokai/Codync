# Stand-in agent CLIs

Scripts named like a real agent's CLI. `npm run dev -- --cdp` puts this folder first on the sandbox host's `PATH`
(`apps/desktop/tools/dev.mjs`), and the host finds an agent by its CLI's name on `PATH`, so each script here is that
agent for an e2e sandbox: installed, signed in by the host's own sign-in command, started by the host's own ACP
command. The host has no code for them. Why and what runs on them:
[test stack: stand-in agents](../../../../docs/guides/test-stack.md#stand-in-agents).

They exist so the desktop e2e suite can walk the screens that need an agent the host knows by name (its sheet, its
sign-in terminal, its login profiles) without starting a real agent or touching a real account.

| Script | Commands | What each does |
| --- | --- | --- |
| `devin` | `devin auth login` | Waits for Enter, then writes `$XDG_DATA_HOME/devin/credentials.toml`: the file that says a Devin login profile is signed in. Only on a login profile (`CODYNC_AGENT_ACCOUNT` set); on the computer's own login it writes nothing. |
| | `devin acp` | A minimal ACP agent: answers every message with `Devin stand-in on <login profile id or "the default login">, signed in` or `signed out`, so a spec reads from the chat which login the bot's agent runs on. Signed in means that login profile's file exists; the computer's own login always reads signed out, whatever a real Devin has there. |

Try one by hand:

```sh
host/tests/fixtures/bin/devin auth login          # "The stand-in signs in login profiles only."
CODYNC_AGENT_ACCOUNT=a1 XDG_DATA_HOME=/tmp/standin host/tests/fixtures/bin/devin auth login
```

## Adding a stand-in

1. Pick the agent in the host's catalog (`host/src/agent/backends/catalog.rs`, and for login profiles
   `host/crates/accounts/src/agents.rs`) whose sign-in and login storage match the screen under test. Prefer one
   that needs no CLI call to tell whether it is signed in (a login file), and that has its own ACP mode (`local`), so
   the host starts the script and not a registry build.
2. Name the script after the first of the agent's `bins`, start it with `#!/usr/bin/env python3`, and make it
   executable (`chmod +x`).
3. Answer exactly the commands the host runs for that agent (`login`, `local`, any `signed_in` or `sign_out`), and
   exit with an error on anything else, so a new host call shows up as a failed test.
4. Write only inside the folders the host's variables point at for a login profile. Never write in the user's home,
   and never on the computer's own login.
5. Add its row to the table above (the test stack guide links here), and say in the guide what runs on it.

The scripts don't run on Windows (no `#!` line there): `dev.mjs` leaves `PATH` alone and their specs are skipped.
