# Marketplace and agent setup

The Plugins screen configures capabilities installed on the selected computer. Account membership does not install the same capability on every computer.

| Capability | Implementation | Behavior |
| --- | --- | --- |
| Agents | `host/src/backends.rs`, `host/src/registry.rs` | Detect installed harnesses and resolve ACP registry adapters |
| Connectors | `host/src/market.rs` | Discover MCP servers or add one manually; install per computer and enable per bot |
| Connected apps | `host/src/composio.rs` | Expose connected Composio apps as connectors |
| Skills | `host/src/market.rs` | Instruction folders containing `SKILL.md`, stored under `~/.codync/skills/<id>` |

An enabled connector is passed to the agent as an MCP server when its session starts or resumes. An enabled skill tells the bot where its instruction folder lives. These are distinct from the built-in `team` and `computer` MCP services.

Registry and repository metadata are untrusted: the host validates names and paths before installing. Secret values stay on the computer; listings report whether required values are configured. Adapter downloads and external authorization depend on the selected provider.

To check a change, use a disposable bot on the intended computer, verify discovery/install/configuration, start a new session as needed, and confirm the agent actually receives the capability. Listing an item alone does not verify its authentication or runtime behavior.

See [file structure](../architecture/file-structure.md), [bot collaboration](bot-collaboration.md), and [remote screen](remote-screen.md).
