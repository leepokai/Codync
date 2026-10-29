# Error tracking detail

What PostHog could record so a production error can be acted on without asking the user, and
what each step costs in privacy. A1 and A2 are built (2.19.2); the rest is a proposal. Today's
rules and payloads:
[usage analytics](../features/analytics.md#error-tracking). The 2026-10-19 triage that prompted
this: [PostHog error triage](../investigations/posthog-errors-2026-10-19.md) (two of its eleven
findings stayed unexplained for lack of detail).

## What's missing today

| Client | What an error report carries | Gap |
|---|---|---|
| Host | Panic location (file and line) and a literal panic message; release builds are stripped, so no frames | Which code path led there; a formatted panic message (`failed printing to stderr: …`) is dropped |
| Desktop | Type, mechanism and readable frames (the renderer is built unminified) | A `DOMException` or a rejection has no stack, and no message is sent, so `NotFoundError` can't be placed |
| iPhone | MetricKit type, signal and code addresses, symbolicated by PostHog (A1) | — |
| Android | Type and frames (class, method, file, line) | — |
| All | The session it happened in (A2) | — |

The public promise (`web/app/privacy/page.tsx`, *Crash and error reports*) says an error's message
is never sent and paths are cut down to Codync's own files. Options A keep it; B and C change it.

## A. More detail without changing the promise

1. **iPhone frames symbolicated by PostHog** (built, 2.19.2). Send MetricKit frames as PostHog
   `apple` frames (`instruction_addr`, `image_addr` = `address − offsetIntoBinaryTextSegment`,
   `module`, `in_app`)
   with an event-level `$debug_images` list (`debug_id` = the frame's `binaryUUID`, `image_addr`,
   `type: macho`). Each release's dSYMs are uploaded from its Xcode Cloud archive by the *iOS
   dSYMs* GitHub workflow after *Submit iOS* (`tools/upload-dsyms.py`, `posthog-cli` on macOS,
   a personal API key with *error tracking write* as the `POSTHOG_CLI_API_KEY` secret), rather
   than from an Xcode Cloud script: the secret stays in GitHub and an older version can be
   uploaded by hand. PostHog then shows function, file and line for every iPhone crash. Only
   addresses and binary UUIDs leave the device. The new frames group iPhone crashes into new
   issues from 2.19.2 on.
2. **Sessions** (built, 2.19.2). A `$session_id` (UUIDv7, new after 30 minutes without events or
   24 hours, and on each launch of an app or start of the host) on every event lets PostHog's
   error page show the feature events that led up to an exception; an iPhone crash, reported on
   the next launch, goes in the session the crashed launch ended in. Those events are already
   sent; nothing new is recorded.
3. **Context properties on `$exception`.** The screen or sheet that was open, the call engine, the
   connection (direct or relay), the agent of the bot involved: catalog names, as other events carry.
4. **Host frames.** The same server-side symbolication for the host: capture instruction addresses
   and the binary's build ID on panic, and upload split debug info from the release workflow for
   macOS, Linux and Windows. The most work for the least gain (panics already carry their location).

## B. Scrubbed messages (changes the promise)

Send an error's message after removing paths, URLs, e-mail addresses, quoted strings, long tokens
and numbers. It explains most errors at a glance (`Requested device not found`, `Broken pipe`), but
messages from agents and the OS can still carry a user's content in shapes a scrubber misses. Needs
the privacy page and the four in-app analytics texts updated (listed in [usage analytics](../features/analytics.md)).

## C. A problem report the user sends (changes the promise, with consent)

A *Report a problem* action that shows exactly what will be sent (recent errors with messages, the
host log tail, versions) and sends it only when the person confirms. Full detail, explicit consent,
but UI in every client.

## Recommendation

A1 and A2 first (done): the largest gain (readable iPhone crashes, the steps before every error)
with no change to the promise. Then C if a bug needs a message to be understood. Avoid B.
