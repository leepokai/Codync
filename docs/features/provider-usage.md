# Provider plan usage

The host owns quota inspection; desktop, Android, iOS and TUI render its `usage`
report. Percentages describe a provider's plan window, not task completion,
context occupancy, token totals or a billed amount.

Claude asks its ACP adapter's own `/usage` report first (the account that
session is signed in to), then the CLI's `/usage` when the adapter cannot
answer; the status-line ingest and the adapter's rate-limit event still update
the same windows. Codex asks its app-server
(`account/rateLimits/read`) and falls back to the newest local rollout's
`rate_limits` when the app-server cannot answer. Grok starts its CLI's ACP
server and calls `_x.ai/billing`; the current period's share used becomes the
window (weekly or monthly), and a period with no recorded usage is zero, not
unknown. Antigravity inspection starts an isolated ACP session without MCP
servers or filesystem/terminal capabilities. It rejects every client tool
request, caps response text at 64 KiB and times out each request after 20
seconds. `/usage` is sent only when the adapter's initialization advertises a
stable version of at least 1.4.1. Unsupported or unknown versions receive no
prompt. The inspection process is stopped on exit.

The Antigravity report is shown as **Gemini** (the Google plan it reads); an
isolated account uses provider ID `antigravity:<account ID>` and its saved
display name. Reports and model groups remain separate. Percent remaining
becomes percent used. A missing reset stays unknown; valid UTC resets become
epoch milliseconds. API-key accounts may have no plan quota. Invalid/missing
quota never becomes a fabricated zero.

`status` is an optional additive field: `unsupported` asks to update the adapter;
`unavailable` means the current account could not report a quota. These reports
have no numeric windows. A signed-in agent that reports nothing still gets a
placeholder (`unavailable`) so its counters don't vanish. Older clients ignore
the field. A failed observation replaces the old windows rather than presenting
them as fresh. Deleted profiles are removed from the report on refresh.
Concurrent refresh requests wait for the active refresh and then return its
result, so a manual refresh never answers with a half-filled list; the existing
five-minute polling interval is retained. The newest report is saved in
`usage.cache`, so a restarted host shows the last reading while the first probe
runs.

The island pages through two providers/accounts at a time and scrolls windows
inside each card. General desktop, phone and terminal usage views already list
all reports. Their unavailable states and Antigravity source labels match; the
island's pagination is desktop-only because the other clients do not have its
small fixed window. The user clarified that "web" means the Electron desktop,
so the public website is unaffected.

Source tests cover version gating, invalid/missing quota, conversion and model
groups, distinct account identities, Codex app-server parsing, Grok's proto3
billing periods, the saved reading and Android decoding of unavailable
accounts. Real account/provider and installed-app checks must be recorded
separately; compilation does not establish live quota availability.
