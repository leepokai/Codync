# GPT-Live independent review fixes

PR: https://github.com/leepokai/Codync/pull/258
Baseline: `c805a151d71dd5c257d72f25a72bc495b8da6eba`.

## Changes

- P2 event backlog: the host now subscribes the Live WebRTC data channel to session start/close/errors, input/output transcripts, and only the Responses lifecycle/completed-item events consumed by its protocol. Unused parameter/text token deltas are excluded at the provider, so they never fill desktop/iOS ordered RPC queues or delay Android's queue. This uses the documented nested `response_event` selector.
- P2 restricted keys: failed model discovery falls back to minting a real Realtime credential against the saved default. Failed credential validation still prevents saving. Live-only model discovery still uses model-access validation.
- P3 transcript handoff: storage and handoff share the same speaker, pause and session continuity check. Separate turns retain separators, while byte-limit splits preserve exact words and CJK text.
- Additional confirmed P2: OpenAI key discovery and validation share a 20-second deadline before vault writes. Successful discovery is reused for defaults; saving no longer waits for a redundant model-list request. This avoids client timeout after a successful save. Gemini's existing flow is unchanged.

## Verification

- 16 voice tests passed, including restricted model-list access, failed mint rejection, Live-only access, reused defaults, discovery/mint deadlines, and the event-subscription contract.
- 38 call tests passed, including separate speaker turns, long pauses, reconnect epochs, and long UTF-8 utterances crossing storage lines.
- Rust format check, all-target Clippy with warnings denied, and host build passed.
- Three independent read-only review perspectives completed. A separate verifier confirmed the additional timeout finding; correctness and completeness reviewers rechecked the fix and found no remaining confirmed defect in this patch.
- All runtime edits are in the shared Rust host. Desktop, iOS and Android receive the same subscription configuration; no client UI or transport implementation changed. The existing 2.32.0 release bump remains sufficient.

## App acceptance

Pending final sandbox call after the user's macOS Keychain approval for the rebuilt host. The previous build was stopped before replacement. No fake microphone flags or security bypasses were used.

Restricted API permissions and slow requests were tested with injected provider outcomes; no existing API key permissions were changed. Physical-phone audio, long calls and forced network drops remain outside this follow-up acceptance.
