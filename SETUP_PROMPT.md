# Set up Codync with an agent

Give this file to a coding agent (Claude Code, Codex, Cursor…) on the computer you want to
use, and ask: **"Follow SETUP_PROMPT.md to set up Codync here."** It installs the host and the
desktop app, checks they run and tells you how to pair a phone. Everything it runs is listed below,
so you can read it first.

## Rules for the agent

- Work on **this computer only**. Don't touch other machines, accounts or services.
- Run only the commands in this file. Before any step that needs `sudo` or an administrator
  prompt, tell the user what it will do and wait for their yes.
- Never delete, move or print anything in `~/.codync` (Windows: `%USERPROFILE%\.codync`): it holds
  the user's bots, conversations and tokens. Never print the contents of `~/.codync/token`.
- If a step fails, show the error, say what you tried and stop; don't work around it.

## 1. Check what is already there

```bash
codync-host status
```

If it prints that the host is installed and running, skip to step 3.

## 2. Install

Pick the user's system:

- **macOS or Linux**

  ```bash
  curl -fsSL https://raw.githubusercontent.com/leepokai/Codync/main/packaging/install.sh | sh
  ```

  Add `sh -s -- --host-only` for a server or a Mac without a screen. Re-running upgrades.
- **Windows**: download
  `https://github.com/leepokai/Codync/releases/latest/download/codync-windows-x64.exe`, then run
  it. It installs for the user and starts the host at sign-in. The installer isn't code-signed
  yet: SmartScreen asks once (**More info → Run anyway**); tell the user to confirm it.

Then register the background service (the app does it on first launch; a host-only install
needs it):

```bash
codync-host install
```

## 3. Check it runs

```bash
codync-host status
```

It must say the host is installed and running. If not, run `codync-host serve` in a terminal and
read the first error.

## 4. Agents

Codync uses the coding agents already installed and signed in on the computer (Claude Code,
Codex, Pi…); nothing to configure here. Claude Code, Codex and Pi run through adapters fetched by
`npx`, so Node.js must be installed for those. Tell the user which agents you found and which
need signing in; don't sign in for them.

## 5. Pair a phone (optional)

```bash
codync-host pair
```

It shows a QR code and a link. Tell the user to scan it from the Codync app on their phone
(Android or iPhone). Remote access works through the encrypted Cloudflare relay with no extra setup.

## 6. Report

Say, in a few lines: the version installed, whether the host runs, which agents were found, and
whether pairing is ready. Don't paste tokens, QR contents or environment values.

More: [README](README.md#install) · [Linux servers](docs/guides/linux-servers.md) ·
[Remote access](docs/reference/remote-relay.md).
