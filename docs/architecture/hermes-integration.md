# Hermes integration

The reference checkout is NousResearch/hermes-agent at
`10d25f8ad65fdf71304e9272959421a0c22e5d23`. Import tools refuse a different
commit or modified tracked files. Nothing in the imports executes skill scripts.

## Sources and adaptations

- `acp_adapter/elicitation.py`: the host recognizes Hermes' `clarify-*`
  permission fallback and leaves its choices to the person even when tool
  approval is automatic. Permission option ids and names stay unchanged.
  The existing ACP actor handles tools, approvals and `session/cancel`.
  The one-click installer now downloads the bootstrap at the same full commit and
  passes that commit to the upstream installer. Upstream PM supplies its managed
  Python and hash-verified dependency locks; optional provider extras remain lazy.
  Browser/computer-use payloads are skipped because Codync already owns those.
  Existing CLI installations are still detected without forcing a reinstall or
  replacing their provider credentials; their runtime revision needs separate
  verification before a pinned-runtime acceptance claim.
- `skills/` and `optional-skills/`: 185 complete skill directories, stored in
  `host/src/market/hermes/skills.tar.gz`; their instructions and auxiliary files
  are unchanged. The generated JSON contains their original directory paths.
  Codync folder ids have a `hermes-` prefix and a stable path hash to avoid
  collisions with OpenBot and the person's skills. Names inside SKILL.md stay
  unchanged. Each installation includes `HERMES-LICENSE`, the upstream MIT
  license. Installation uses the host's skill store and never overwrites an
  occupied folder or another source. A failed installation is removed before
  it can appear in the installed list.
- `agent/skills_index_delta.py`: Codync already compares the skills block as
  part of `chat::context::Identity` and sends the complete updated block on the
  next turn, including after compaction. That implementation is reused.
- Skill improvements: `save_skill` makes a bot-owned copy of an enabled Hermes
  bundle, retaining its supporting files and license. Other bots keep the
  original. Changes keep up to 50 numbered versions; `skill_history` reads them
  and `restore_skill` restores one only if `ifVersion` still matches. A failed
  database publication restores the previous file. Removing a skill removes
  its version history too. Complex-task skill proposals already exist in
  `chat::skill_proposals` and continue to require user review.
- Session search (`agent/runtime_session_store.py` and session persistence):
  Codync's entry store now maintains an FTS5 trigram index. A single transaction
  creates its triggers and backfills existing messages. Inserts, edits and
  deletes update the index in the same database transaction as the entry.
  FTS narrows candidates for ASCII words of three or more characters; existing
  Unicode substring checks, bot isolation, internal-message exclusions and
  final-reply checks still decide the result. Short and Unicode queries retain
  the previous substring behavior. `search_history` accepts inclusive `since`
  and exclusive `until` timestamps in Unix milliseconds for dated recaps.
- `optional-mcps/*/manifest.yaml`: all 66 manifests are represented in
  `host/src/market/hermes/apps.json`, retaining server names, URLs, authorization
  modes and input variable names. Browsing adds only services absent from
  OpenBot by server name or endpoint. Existing user connections are marked
  installed and are never rewritten. OAuth uses Codync's existing PKCE and
  vault implementation; n8n asks for its instance MCP URL. Asana asks for the
  person's own registered MCP app credentials and documents the host and mobile callbacks
  instead of assuming support for dynamic client registration.
- `cron/blueprint_catalog.py`: all 16 slot schemas and prompt templates are
  extracted with a restricted AST reader, without executing the Python module.
  `routineTemplates` / `routineTemplateDraft` and the corresponding MCP tools
  return validated, disabled drafts for the existing scheduler. Unknown fields,
  invalid enum values, times and zones, and delivery without a destination are
  rejected. User-provided braces remain literal. Named weekdays are accepted
  for the upstream competitor-watch default; 60-minute and 24-hour cron steps
  are normalized to hourly and daily schedules.
- `cron/delivery_queue.py` and `delivery_outcome.py`: the existing platform outputs
  use a SQLite outbox keyed by run and destination position. The flow snapshot is
  saved with the terminal run and acknowledged after enqueue. Recovery reads
  existing step outcomes and idempotently inserts missing deliveries. Four workers
  bound sending; missing credentials and failures detected before sending defer
  without repeating the task. Claims surviving a restart become `unknown`, and
  ambiguous network failures are not retried automatically. Terminal rows erase
  their report/event payload and remain as idempotency tombstones. Stop, pause,
  removal and link disconnection revoke queued deliveries. This currently covers
  the existing Slack, Telegram, Discord and GitHub outputs and now WhatsApp.
  WhatsApp uses the existing linked-device bridge and requires the linked sender
  account id separately from its recipient. Its binding is captured at routine
  turn start (or explicit flow test), retained in the durable run/flow event and
  reused unchanged at enqueue and recovery. Binding capture preserves `source`
  and other trigger fields, including explicit tests of paused routines. A private
  per-run marker prevents `Starting` recovery from capturing authorization again. A
  logout or account replacement invalidates older reports. Temporary disconnect
  before writing is typed as unsent and deferred; a missing acknowledgement after
  writing is ambiguous and never retried automatically. No new credentials or
  parallel WhatsApp session are created. The MCP routine schema advertises the
  destination. Desktop and Android output pickers now include WhatsApp, with a
  separate linked-account field. iOS now reads and edits the same output models
  and sends optional changes through both typed save overloads. The terminal's
  output editor uses bounded, quoted text parsed only by the host with the existing
  `shlex` dependency. It preserves fields it cannot represent when unchanged.
  `saveRoutineWithOutputs` is an additive intent so an older host cannot silently
  ignore destination changes. All clients omit unchanged outputs from ordinary
  saves, and use that new intent only for explicit output edits. The terminal
  drawing lives in a separate module with a compact small-window layout.
  Editors retain an opening snapshot instead of comparing against live refreshed
  props. Explicit changes send `ifOutputs`; the host checks it under the routine
  state lock before modifying anything. Concurrent destination changes therefore
  fail with a reopen instruction, while unrelated edits preserve newer outputs.
  Swift animated rows also guard their construction against retained stale indices.
  Hermes's `deliver=local` is refused until save-only delivery exists.
- Messaging voice notes: `tools/audio_container.py` is ported to
  `voice/transcription/container.rs` with its MIT source pin. Container bytes, rather
  than platform MIME hints, select the upload type. Calls retain their existing WAV
  API. Telegram and WhatsApp reuse the voice vault, existing OpenAI/Gemini defaults
  and host transcription path; there is no second provider credential store. A
  bounded in-memory memo coalesces matching audio/provider/model requests within
  the same credential, endpoint and spelling context (only their hash is retained), with
  four concurrent transcriptions and a 20 MiB input limit. Raw audio is not cached
  there. Unsupported formats or provider failures produce explicit failures.
  Telegram downloads only authorized private-chat attachments through its existing
  connection. Durable per-account metadata queues keep control commands and
  approvals responsive while speech requests wait. WhatsApp uses the existing
  media directory and queues metadata without base64 audio. Temporary disconnect
  keeps pending metadata; unlink and authorization generations revoke it. Both
  channels check the fence again at publication and at actor turn start; checked
  database reads fail closed on SQL errors. No renderer business logic was added.
  WhatsApp roots are now scoped to chat and bot, preserving old roots with a
  transaction. Permission cards retain their exact input entry ids so a later group
  participant cannot inherit another author's approval. Mixed-author decisions
  remain in the Codync app. Every ACP option, including Hermes clarifications, is
  available by its original id through the numbered reply. Signal, Email, Home
  Assistant use the additional native channel bridge described below; Slack and
  Discord voice notes use the same host transcription path.
  WhatsApp sends now wait for the existing bridge's matching `sent` event and a
  nonempty message id. The receipt reader stays independent of slow ingress and
  control responses. Timeouts, disconnection and cancellation remove waiters;
  they never become delivery success. Replies and approval cards reserve their
  durable receipt before sending and do not repeat an ambiguous send. App-only
  cards have separate exclusion records so older decisions cannot starve newer
  valid ones. Bridge lifecycle epochs reject input from a replaced reader. A
  bounded raw-input queue reports backpressure to authorized senders, while the
  durable inbox handles accepted metadata across temporary disconnection.
  `host/whatsapp-bridge/bridge.mjs` now downloads attachments as bounded streams
  (32 MiB), retains their metadata on failure and recognizes audio context for
  group mentions/quotes. This bridge transport is reused by existing callers;
  no new connection or credential is introduced.
  Logout (including a provider-reported logout) and account replacement erase
  conversation routes and approval receipts in the same transaction as generation
  revocation. Chat entries and outbox/exclusion tombstones remain. New roots include
  the link's generation, so restoring a mapping cannot reactivate an older account's
  replies. Normal reply/card batches capture authorization and select entries under
  one connector lock; text and files both use that binding at write time. Card
  reservations recheck the same binding under the lock to prevent late publication
  after logout. Failed revocation prevents a subsequent start until it succeeds.
- Tool RPC (`batch_runner.py` and the agent RPC pattern): `codync-host tool`
  accepts a bot id, a built-in MCP family, a tool name and bounded JSON arguments
  from a file, stdin or the command line. It calls the existing MCP-to-host path,
  preserving validation, approval cards, output limits and credentials. It never
  retries a mutating request. Scripts can chain calls without another agent turn;
  tools keep their own provider/service costs. Team transport deadlines now cover
  the actual ten-minute task budget and thirty-minute whole-pipeline budget rather
  than cutting them off at thirty seconds. This is a local CLI entry point to
  existing host intents, with no new renderer business logic or client API fields.
  The CLI deliberately exposes seven built-in bot-tool families. Composio app
  execution keeps its existing immutable approval/receipt-aware MCP bridge;
  external network agents keep their separate identity-based bridge. Neither is
  redirected through the generic CLI call.
  `ask_bot` additionally accepts `timeoutSeconds` (1–600, default 600), covering
  queue time and execution. It reuses the existing request-specific cancellation
  guard. Synchronous asks, deferred messages and external API requests now share
  admission accounting: 16 pending requests per sender and 64 on the host. A bot
  cannot bypass its pending quota by mixing queued messages with synchronous asks.
  New regressions cover mixed admission, capacity release, deadline validation and
  the public ask deadline while unrelated recipient messages remain queued.
- Environment credential inheritance: the existing host `launch_env` already
  passes per-bot variables to the native Hermes ACP process, retaining Hermes'
  `TERMINAL_ENV`, `TERMINAL_SSH_HOST` and related names. Saving a bot override
  previously copied the merged provider environment into its own vault slot,
  freezing inherited credentials. Updates now read and write only that scope,
  under the connector lock; names distinguish actual bot overrides from inherited
  provider keys. Existing explicit overrides are preserved, never silently removed.
  Written regressions cover separate SSH/Docker bot settings, provider-key rotation
  and removing an explicit override. The native runtime bridge now launches
  `HermesACPAgent(SessionManager)` as the execution owner; it does not send backend
  variables to a gateway viewer and assume that the gateway adopted them.
- Native runtime sources: `acp_adapter/{entry,server,session}.py`,
  `tools/terminal_scope.py`, `hermes_cli/{config,profiles}.py`,
  `agent/delegation_provider.py`, `plugins/delegation/default/__init__.py`,
  and `tools/delegate_tool_config.py` (MIT, pinned commit above).
  `host/src/agent/hermes/{acp_owner,state_probe,delegation_routes}.py` adapt these
  seams without modifying the reference checkout. Host-owned settings have one
  frozen revision and one Apply on desktop, iOS, Android and terminal. A conflicting
  Apply is refused. The process receives the same complete snapshot that produced
  its launch command; account environment cannot replace it. UI changes restart
  the owner after its active turn. The terminal fields scroll to keyboard focus.
  Installation status verifies the unchanged source commit and the installation's
  own Python 3.11–3.14 virtual environment. Profile inspection reads metadata only;
  it does not authenticate, start remote backends or report connection health.
  Explicit named profiles and unreadable sticky-profile files fail closed.
  A separate, explicit `hermesEnvironmentHealth` intent checks the saved backend
  through the native factory, without creating a model session. Its revision is
  checked before and after the probe. All four clients expose this action and
  say it may start a sandbox; installation refresh never starts one. Probe
  environments have private task ids, bounded lifetimes and their own cleanup.
- H3 native memory: builtin file memory remains Hermes's own implementation;
  Codync's existing persistent per-bot memory/Dreamer and dated FTS history are
  reused. Optional Honcho is loaded through the original plugin interface and
  existing Hermes profile. Selecting Honcho requires that profile to already name
  Honcho and its plugin to initialize; failed initialization is an error, never a
  silent fallback to builtin memory. No Honcho tokens are copied to a second vault.
- H4 native channels: the pinned Signal and Email platform adapters run as
  transport-only subprocesses; the host owns routing, durable admission/outbox,
  pairing, author-bound approvals, generation revocation, files and transcription.
  Native profile configuration/credentials are reused without a second gateway
  agent. Apps install through the existing generic configuration forms on all
  clients, including an initial one-use pairing code and sender policy. Additional
  status/pair/revoke methods are documented; dedicated management buttons were
  not added. Email also has a signed, bounded, deduplicated RFC822 webhook.
  Outgoing SharedFile ids resolve to bot-owned immutable snapshots, checked by
  size and SHA256. Incoming attachments enter the standard host uploads path.
  Home Assistant uses the official plugin pin below with explicitly selected
  entities. Exact source paths, permissions and transport behavior are in
  `docs/integrations/hermes-h4-native-channels.md`.
- H6 native environments: the execution owner binds the complete profile terminal
  scope during agent construction, turns and shutdown. Local, Docker, SSH, Modal,
  Daytona and Vercel Sandbox use the pinned native factories, policies, credentials
  and lifecycle. Managed configuration and locked fields remain authoritative.
  Docker uses the native project-to-/workspace mount. Serverless suspend/persistence
  behavior remains provider controlled; no zero-cost guarantee is invented.
  `ssh_owner.py`, `ssh_workspace.py` and `ssh_processes.py` adapt
  `tools/environments/{ssh,file_sync}.py`, `base_output.py` and
  `tools/process_registry.py` (MIT, same pin). SSH gets private control sockets,
  authenticated remote controllers, and a heartbeat lease; generic remote targets
  require Python 3 and POSIX waitid(WNOWAIT). Only project files synchronize:
  credential folders, environment files, symlinks and unsupported archive paths are
  excluded. Pulling checks conflicts instead of overwriting concurrent host edits.
  It rechecks each expected target after staging, immediately before replacing or
  deleting. Ordinary filesystems cannot make that final comparison/replacement
  atomic against an adversarial writer in another process.
  Native background jobs retain cancellable handles instead of detached nohup jobs.
  Stop closes the owner and preserves resumable conversation identifiers. Remote
  lease expiry bounds process cleanup after a broken connection; it cannot promise
  file recovery while that connection is unavailable. Deliberate process-group escapes
  require OS/container containment beyond this adapter.
- H7 native routing: saved provider/model routes expose only opaque `codync_route`
  enum values to the agent. Raw credentials/endpoints are never model parameters.
  Each child resolves its chosen native provider through existing credentials;
  choosing a different provider clears stale endpoint/key/fallback hints in the
  process overlay, without rewriting the user's profile. Hermes enforces independent
  child iteration budgets, shared concurrency and child inactivity bounds. The
  inactivity setting is not a wall-clock or monetary budget. The owner uses the
  native joined-delegation variant, so children remain in the turn and their summary
  returns through the parent. Existing host ask/batch/pipeline queue limits and
  absolute deadlines remain separate and the script RPC reuses the host connection.
  `delegation_quota.py` wraps each child's real conversation call with OS-owned
  permits shared by the bot's chat and routine owners. A timed-out native waiter
  cannot release a permit while its inference thread still runs; process death
  releases the OS lock. Top-level children wait within a bounded cancellable queue.
  Nested children refuse a full pool instead of deadlocking a joined parent.
  Slot files remain in place so deleting/recreating an inode cannot split the quota.
- H8 voice: native NeuTTS 1.4.1, Jo reference and cache-only inference stream
  bounded PCM frames through an owned process and caller-scoped RPC. Desktop,
  iOS and Android preserve their existing STT connections and choose local output
  separately. Sustained speech interrupts playback and pending inference. Mobile
  Hey Hermes is optional, on-device and foreground-only; it uses native speech
  recognition rather than pretending the upstream TFLite runtime was ported.
  The terminal has no audio device layer and remains text-only. Exact source and
  framing/cancellation contracts: `docs/integrations/hermes-h8-voice.md`.
- `gateway/platforms/api_server_openai_routes.py`: the host exposes local
  `/v1/models`, `/v1/health`, `/v1/pricing` and `/v1/chat/completions` routes.
  `openaiCreateKey`, `openaiKeys` and `openaiRevokeKey` are local-only admin
  methods. Keys grant access to selected bots and are returned only on creation;
  their hashes and grants are encrypted in the vault. Requests go through the
  normal bot queue and keep its tools and approvals. Client disconnect or timeout
  cancels only that queued or active request. Text messages are supported; external
  tool schemas are rejected. Streaming sends keep-alives and a final content
  delta, followed by `[DONE]`. Unknown provider prices are null, never zero.
  The supported completion fields are `model`, `messages`, `stream`, `n=1`
  and empty `stream_options` (or `include_usage=false`). Unsupported token
  limits, sampling settings and response-format constraints are refused rather
  than silently ignored.

## Regeneration and verification

From the task checkout, with Python and PyYAML installed:

```powershell
python tools/import-hermes-skills.py ..\ref-repos\hermes-agent
python tools/import-hermes-mcps.py ..\ref-repos\hermes-agent
python tools/import-hermes-blueprints.py ..\ref-repos\hermes-agent
python tools/verify-hermes-imports.py ..\ref-repos\hermes-agent
```

The skill archive has deterministic timestamps. Verification compares all 185
skills and 1,281 archive members byte for byte with the source and checks the 66
MCP manifests, including their input names, and the 16 blueprints. Only files
tracked at the pinned commit are eligible; local ignored files and secrets are
not imported. Tests in `market::hermes`,
`market::hermes_apps` and `store::session_search` cover extraction, catalog
identity, query behavior, visibility, edits, deletions and backfill.

## Acceptance work still required

### Current validation constraint

On 2026-11-10 the five remaining code blocks were connected: native memory/Honcho,
Signal/Email/Home Assistant, owned environments/project SSH synchronization,
saved subagent routes/shared quotas, and local progressive voice/mobile wake.
Three independent source reviews covered correctness, completeness and refactoring.
Confirmed fixes include the unsafe native entry import, stale provider overrides,
SSH conflict rechecks, published attachment resolution/cleanup, real backend health,
and stale iOS host responses/notices. Follow-up read-only verification confirmed
the fixes. Rustfmt parsed/formatted 141 changed Rust files; TypeScript and Python
sources were parsed as ASTs. These are source checks, not builds or executed tests.
No new app, service check, model, dependency installation or production action ran.
The regression tests are present but remain unexecuted under the user's constraint.

On 2026-11-09 the user requested continued code work without compilation. No
compilation, packaging, installation or app E2E was performed after that request.
The current source passed rustfmt parsing/formatting and `git diff --check`;
the WhatsApp bridge passed `node --check` without running the bridge. An isolated
in-memory SQLite probe executed the actual WhatsApp schema, migration and card
selection SQL and passed legacy retention, repeatable initialization, bot
isolation and exclusion pagination. These do not validate Rust types or the
live messaging protocol. The receipt, reader-replacement and socket-close
regression tests were written but not executed. Further regressions cover an
isolated speech HTTP endpoint, immutable speech request context, `Starting`
recovery, explicit test events, late enqueue after relink and loaded outbound
text/file batches. These have not run. The combined receipt-reader/blocked-ingress
E2E flow remains untested.
The pinned Windows installer command passed PowerShell AST parsing without being
executed. Import verification still passed for 185 skills, 1,281 archive files,
66 MCP manifests and 16 blueprints. The reference checkout remains at the pin.

The output-editor phase additionally passed the installed Babel parser for its
TypeScript/TSX sources and the interface detector (no findings). This was syntax
inspection, without transforming or executing the app. Swift and Kotlin builds,
their new tests, the host tests and all new app flows remain unrun. Independent
reviews identified a moved-module path, stale Swift row indices, a missing
terminal warning and live-prop output comparisons; each was fixed. Two reviewers
then exhausted their service quota; subsequent verification used a manual reading
pass rather than claiming complete independent review. A host regression now
covers preserving newer outputs on unrelated edits and rejecting stale replacements
before modifying the routine. Authentication regressions use an isolated in-memory
vault key and never access Codync Dev's real credentials.

The Slack/Discord audio phase passed rustfmt parsing/formatting and a separate
in-memory SQLite probe using the actual schema, migration and pending-message
SELECT statements. It preserved a legacy queued message, repeated initialization
without losing it, and read back the new audio metadata. This is a SQL probe,
not an execution of the Rust migration or the provider download/transcription flow.
The newly written voice-only ingress, metadata, URL, authorization and migration
regression tests remain unrun under the same no-compilation constraint.

Read-only reviews found and led to fixes for delivery intent persistence,
history rotation, explicit test runs, pre-send failure classification, late
enqueue after Stop, saved step recovery after a bot becomes hidden, and the
pipeline transport budget. Added tests cover skill-history retention, delivery
claims/outcomes/cancellation/recovery and the CLI protocol through an isolated
mock host. These tests have **not run**. Earlier passing tests predate the outbox,
CLI, installer and messaging changes and do not establish that this current source
builds. New messaging regressions cover authorization read failures, revoke and
re-authorize, per-bot WhatsApp threads, legacy migration, approval authorship,
durable queue order and disconnect retention. They have **not run** either.
Composio and external-agent CLI families are intentionally outside the generic
tool command, retaining their existing approval-aware MCP paths.

These imports do not establish completion of H1–H10. In particular, an installed
Hermes CLI is a separate runtime and must be verified against the required
version before claiming a pinned sidecar test. A passing dependency check or
catalog listing does not prove an authenticated conversation or connection.

- H1: installed Dev conversation, tool execution, approval and cancellation.
- H2: real-app use, reviewed skill creation, improvement and restore checks.
- H3: dated recall with an actual model, saved memory and an existing Honcho profile.
- H4: real-app checks of Signal, Email, Home Assistant and Telegram/WhatsApp/Slack/Discord
  transcription, sender revocation and approvals. Slack and Discord now persist one bounded
  audio reference with the authorized incoming message, accept voice-only events and reuse
  the host speech vault. Downloads use provider-specific HTTPS origins and the existing DNS
  pinning policy, bounded streaming and a deadline; Slack credentials go only to its file
  origin. Generation/binding checks run before, during and after transcription and again
  at actor admission. New ingress, URL, metadata and revocation regressions have not run.
- H5: real-app template/durable-delivery/cancellation and output-editor checks on
  each client. The host/MCP destination, binding snapshots, acknowledged sends and
  desktop/iOS/Android/terminal editors are implemented; new Rust, Swift, Kotlin
  and TypeScript regressions have not run.
- H6: real-app environment selection, project synchronization, remote process cleanup
  and provider-controlled serverless lifecycle checks against configured services.
- H7: real-app saved per-task routes/limits and live tool-RPC script checks.
  Per-task `timeoutSeconds` (1–600, including queue time) now also reaches batch tasks
  and pipeline stages. The complete task list is validated before dispatch; a pipeline
  uses the smaller of its stage deadline and remaining thirty-minute budget. Parser,
  budget and no-partial-dispatch regressions were added but have not run.
  Review also found that closing an ACP viewer alone does not stop Hermes's
  gateway-owned work. Request and routine deadlines now send `session/cancel`
  through the normal fifteen-second grace path before forced shutdown. Expiry
  during session setup skips the prompt and settles as cancellation; completion
  clears its cancellation deadline so idle resources can release. Added fixture
  regressions cover batch peer isolation, skipped later stages and setup expiry.
  These tests have not been executed.
  Three read-only reviews of this deadline increment were completed. Confirmed
  findings about cancellation ownership, setup expiry, stale cancellation deadlines,
  premature profile acknowledgements and the sender-stop documentation were fixed.
  Missing runtime regressions were added, and the fixture's team-tool contract now
  includes bounded asks/batches/pipelines. Manual source verification and rustfmt
  do not replace executing those regressions.

- H8: real-app NeuTTS, opt-in foreground mobile wake phrase, streaming and interruption.
- H9: real-model conversation, tool execution, approvals, disconnect cancellation and revocation checks.
- H10: connection tests and real-app origin/connector lifecycle checks. Origin links
  are implemented in desktop, iOS and Android; the terminal uses the same catalog
  through its existing connector setup flow. Shared category ids keep imported
  entries visible in category filters.

The Home Assistant adapter is external at this Hermes revision. Its catalog pins
`NousResearch/hermes-official-plugins` to
`e7a218f26db88426ab3e7376f0e000b6e356f5e7`, subdirectory `homeassistant/`
(MIT). That exact object was fetched to the separate reference repository
`../ref-repos/hermes-official-plugins` without checking out or executing plugin
code. Its adapter is integrated as a transport, and its tools module supplies the
native URL validator; installing an event channel does not silently grant extra
home-control tools. The original Hermes reference checkout was not changed.

Do not mark any item complete from its presence in this document. Acceptance
requires the code and the corresponding real application checks.
