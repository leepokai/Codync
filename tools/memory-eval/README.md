# Memory evaluation

Scripted conversations that check what a bot remembers, run against a real host and a real harness
([design](../../docs/design/bot-memory.md#evaluation)).

- `cases.json`: 52 zh-Hant and English cases. Extraction, multi-session reasoning, temporal reasoning, knowledge
  update and abstention (LongMemEval's five abilities); selective forgetting (MemoryAgentBench); detail that only
  a long-form note can hold; short CJK queries; and instructions planted in pasted text.
- `run.py`: one bot per case. It sends each session's messages, starts a new session (which runs dreaming), then
  asks the probe in a fresh session and grades the reply by keywords, plus what memory may not hold.

Run it on a disposable host, since it creates and deletes bots:

```sh
CODYNC_HOME=/tmp/eval-home host/target/debug/codync-host serve --port 19555
tools/memory-eval/run.py --home /tmp/eval-home --backend claude --jobs 3
tools/memory-eval/run.py --home /tmp/eval-home --only tm,lf-01   # some kinds or cases
```

It prints a pass count per kind and each failure with the reply; `results.json` keeps every reply, entry and note
name. Temporal answers are written as `{d+3:%Y-%m-%d}`, `{next_friday:…}` or `{next_month:%B}` and expanded on
the day of the run.

## Prompt cache

`cache.py` reads Claude Code's and Codex's session logs (`~/.claude/projects`, `~/.codex/sessions`) and prints,
per harness, how much of each conversation's input came from the prompt cache and why the cold calls were cold;
for Claude also the input cost against no cache. It needs no host:

```sh
tools/memory-eval/cache.py                 # the bots of ~/.codync and ~/.codync-dev
tools/memory-eval/cache.py eval-home-bots  # workspaces whose path contains this ("/" and "." read as "-")
```

The latest numbers are under [prompt cache](../../docs/design/bot-memory.md#evaluation).
