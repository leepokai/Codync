# Connector live verification

Run from the repository root on macOS:

```sh
cd host && cargo build && cd ..
python3 tools/check-connectors.py --network --slow
```

The script starts the built host on an ephemeral loopback port with a temporary
`CODYNC_HOME` and cloud access disabled. It uses a separate real Keychain key,
stops its host and MCP proxies, deletes that key, and removes its data directory.
It does not change existing bots, connectors or third-party accounts. npm may
retain downloaded packages in its normal cache. Requires Python 3, Node/npm,
an unlocked macOS Keychain, and network access for `--network`.

Without flags the deterministic checks still use real host processes, HTTP,
SQLite, Keychain and MCP subprocesses. `--network` additionally exercises live
providers. `--slow` waits for the production 90-second handshake timeout.
Failures produce a nonzero exit code; live provider failures are not skipped.

## Recorded result: 2026-10-05, macOS

`python3 tools/check-connectors.py --network --slow`: **15/15 passed**.
The timeout check returned after 90.0 seconds and confirmed the child had exited.
The script successfully removed its temporary host data and Keychain entry.

Also passed `cargo build`, the 26 non-ignored `market::` unit tests, and
`cargo test --bin codync-host platform_keyring_roundtrip -- --ignored --nocapture`.
The latter explicitly runs the OS-keychain test skipped by the normal suite.
Python syntax compilation and whitespace checks passed. This is evidence for
the current local worktree, which includes unrelated uncommitted changes; it is
not a released-build certification.

## Coverage

- Reject incorrect local credentials, then correct them through
  `credentialUpdateConnector` without reinstalling.
- Inject encrypted credentials through the actual `mcp local` proxy and discover
  tools in a real subprocess. Public listings omit credentials and command args.
- Check the connector record is encrypted in SQLite; restart the host and verify
  it can recover credentials through the OS Keychain.
- Import a pasted `mcpServers` configuration and verify the resulting connector.
- Reject a missing runtime and a malformed MCP initialize response.
- Verify authenticated remote MCP using JSON and streamable HTTP SSE responses,
  session ID propagation, tool discovery, a harmless echo tool, and deletion.
- Exercise a local OAuth authorization server: resource/server discovery, dynamic
  registration, PKCE verification, forged state rejection, callback replay
  rejection, 401-driven token refresh, refresh rejection, token redaction, and
  cancellation. No actual third-party account login is involved.
- With `--slow`, leave a connector unresponsive for 90 seconds and verify the
  timeout error and that its process has been reaped.
- With `--network`, install official
  `@modelcontextprotocol/server-filesystem@2026.8.31`, discover its tools, read
  a generated test file, and verify it refuses access outside the allowed folder.
- With `--network`, install the DeepWiki HTTPS endpoint, discover tools, and call
  `read_wiki_structure` for the public `modelcontextprotocol/python-sdk` repository.
- With `--network`, resolve Context7 through the live MCP Registry, install its
  registry-selected pinned npm package, and verify MCP initialization/tool listing.

## Limits and known gaps

This checks the shared host API and MCP execution paths. It does not drive the
iOS, Electron or terminal installation UI, log into real provider accounts,
exercise the phone's OAuth deep link, Composio, 1Password, Linux Secret Service,
or the older separate GET-stream/POST-message SSE transport.

Source inspection on 2026-10-05 found this outstanding workflow issue (the install
forms' retry, which kept the connector saved with the first values, was fixed in 2.25.0:
the host now rebuilds an installed registry connector when new values come in):

- Custom/import installation on iOS and desktop, and marketplace installation in
  the terminal, can finish without running `connectorVerify`. The host enables
  newly installed connectors before readiness is verified. Saving successfully
  is therefore not evidence that a connector can run.

These checks add verification tooling only; they do not change shipped behavior.
