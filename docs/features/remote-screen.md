# Remote screen

Remote screen lets a phone view and control a computer. Signaling travels through the host API (`screenOffer`); video and input use WebRTC. Passing a Cloudflare chat test does not prove that the media connection can traverse the current network. TURN is outside the current remote relay design.

## Boundaries

- `host/src/screen.rs` coordinates access to the local helper.
- `apps/screen/` is the macOS capture/input helper, installed through `SMAppService` and responsible for the OS permissions.
- `apps/screen-linux/` implements the Linux helper using desktop portals and GStreamer.
- Helpers communicate locally through `~/.codync/screen.sock`. SDP is non-trickle; input uses the `input` and `input-fast` data channels.
- Screen access is off by default. Enabling through `setScreenEnabled` requires a loopback caller. Interactive OS permission prompts must be completed on the computer.
- Bots with their computer capability enabled receive the built-in `computer` MCP tools (`host/src/mcp.rs`). Their permission policy still applies.

## Verification

Check screen permission, capture, input, disconnect and reconnect separately on each platform. Repeat over the intended remote network; a local screen preview or encrypted signaling success does not verify remote media. See [Cloudflare testing](../guides/cloudflare-testing.md) for transport checks and [architecture](../architecture/overview.md) for data flow.
