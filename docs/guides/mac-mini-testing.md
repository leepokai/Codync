# Mac mini testing

The Mac mini is the real second computer for Codync Dev: a separate machine with its own host, its own sign-in and its own permissions. Use it when a change needs a computer that is not the development Mac: pairing and sign-in on a fresh machine, the packaged app and bundled host, Codync Screen / computer use grants, remote SSH, and two computers on one account (`codync-network`, relay, accounts). A simulated sandbox on the development Mac does not cover these; [Dev app isolation](dev-app.md) and `npm run dev` stay the default for everything else.

## Build and install

```sh
tools/install-dev-mac-mini.sh            # build, then install
tools/install-dev-mac-mini.sh --no-build  # reinstall the last build
```

When asked to **build and install on the Mac mini**, run this script from the change's checkout. It builds Codync Dev (`npm run dist:mac:dev`), copies the app over the `mac-mini` ssh host, replaces `/Applications/Codync Dev.app`, runs the new bundled host's `install --port 19223` and waits for `/health`. The step list it implements is [Replace an installed macOS build completely](development.md#replace-an-installed-macos-build-completely). Report the `/health` line (version, environment `dev`, binary path and hash) as proof; copying files alone is not done.

## Safety

- It touches only Codync Dev (`com.pokai.Codync.dev`, host `com.pokai.codync.dev.host`, port 19223). Production, other variants and the Dev data directory stay.
- ssh is key-only with a known host key and no prompts (`BatchMode`, `StrictHostKeyChecking=yes`); the address and key live in `~/.ssh/config` (`Host mac-mini`), never in the repo.
- Only the built `.app` is sent. No `.env.*`, `~/.codync/token` or credentials are copied; the Mac mini signs in and pairs on its own.
- Both Macs must be Apple silicon: Codync Dev is built for this Mac's architecture only, and it is not notarized (the script removes the quarantine flag).
- The Mac mini refuses the install unless the zip hash, the code signature and the bundled host's hash match what was built.

## By hand on the Mac mini

- A desktop session must be logged in on the Mac mini (`open` and the launch agent need it).
- The first run needs one manual grant of Screen Recording and the other permissions in System Settings.
- Sign in and pair Codync Dev there before testing; nothing carries over from the development Mac.
- Check the app reached its connected main screen: the script proves the host answers, not what the window shows.
