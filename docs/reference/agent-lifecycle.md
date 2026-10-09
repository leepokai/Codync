# Agent resource lifecycle

The host releases unused agent resources while retaining saved conversation identities, transcripts and context snapshots. The next turn resumes the same conversation. This policy lives in the host; iOS, desktop and terminal clients continue using the existing chat, activity and error events.

## Resource policy

| Control | Bound | Behavior |
|---|---|---|
| Idle grace | Two minutes | Release a resumable agent's entire process tree between turns |
| Live sessions | Two per bot process | Release an inactive session before another allocation |
| Session close | 15 seconds | Recycle a resumable process if close fails or times out |
| Session allocation / model selection | Three minutes each | Return an error without replacing a saved conversation |
| Process reaping | Five seconds | Signal descendants before waiting for their leader |

Idle retirement requires no active turn, pending approval, queued command or work, delegated request, group turn, routine or pending routine completion. Agents without `loadSession` support remain alive so their context is preserved.

New, loaded and forked sessions all count toward the live bound, including partially prepared sessions. When the adapter advertises `session/close`, the host closes an inactive session. Otherwise it recycles the process only when existing conversations can reload. Fork preparation retains its parent until the fork is created; an allocation is never opened over the bound.

Applied system instructions are tracked per live session. A fork or instruction refresh closes the previous allocation before loading its replacement, preventing repeated loads from retaining duplicate tool sets. Loading drops replayed history from the live event stream and restores the selected model.

Failed automatic reload preserves the stored session ID and reports a retryable error. It does not start a fresh conversation or repeat interrupted routine work. Model selection has a three-minute deadline; a failed selection can retry on its known live allocation, including nonresumable sessions. First-prompt profile and thread instructions remain pending in the saved context snapshot until a prompt is written, so retries, other lanes and host restarts cannot consume them prematurely. Forks inherit their parent's frozen instructions.

Explicit **New session** releases the old main allocation while retaining saved thread identities. Nonresumable agents report a visible error when another allocation or refresh cannot preserve their existing conversations. An unconfirmed allocation blocks further allocations until it can be safely released or explicitly reset.

## Descendant ownership

Each ACP launch owns its process tree. On Unix, the agent starts in a separate process group. Exit observation leaves the leader unreaped until its group has been signaled, preventing cleanup from targeting a reused PID. Explicit termination, leader exit and owner drop all release the group, including tool descendants that keep stdio open.

On Windows, the host creates the shell suspended, assigns it to a Job Object with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, then resumes it. This establishes ownership before it can spawn descendants. Explicit termination kills the job; releasing its last handle also kills descendants. Both platforms retain child `kill_on_drop` as a fallback. Connectors must remain in the owned group or job; intentionally detached processes are outside this ownership guarantee. See Microsoft's [Job Objects](https://learn.microsoft.com/en-us/windows/win32/procthread/job-objects).

Abandoned JSON-RPC requests remove their pending response entries when their future is dropped. This covers setup and close timeouts as well as canceled requests.

## Linux service containment

A machine-specific systemd memory limit can contain the host and its descendants while resource trends are observed. For a user service, an administrator may choose:

```bash
systemctl --user set-property codync-host.service MemoryHigh=6G MemoryMax=8G
systemctl --user show codync-host.service -p MemoryHigh -p MemoryMax -p ControlGroup
```

These are example limits, not installer defaults. Choose limits for the machine and workload. `set-property` persists generated control drop-ins; normal upgrades must preserve them. Verify the effective cgroup limits after upgrading. See systemd's [resource controls](https://www.freedesktop.org/software/systemd/man/latest/systemd.resource-control.html) and [set-property](https://www.freedesktop.org/software/systemd/man/latest/systemctl.html#%20set-property%20UNIT%20PROPERTY=VALUE%E2%80%A6).

## Validation and diagnosis

Retirement logs include the bot ID, agent generation, known live session count and reason. They do not include prompts, tool credentials or connector payloads. Measure the service cgroup rather than adding process RSS, which double counts shared pages. Process counts and proportional set size help locate retained descendants.

The regression fixtures allocate observable child and grandchild processes for sessions. Coverage includes 100-thread churn, saved main/thread continuity, two-minute idle expiry, active tools, queued work, approvals, nonresumable agents, forks, repeated instruction refresh, reset, failed and timed-out close, partial allocation failure, reload failure, leader exit and actor panic.

Short regression runs demonstrate bounded allocations and cleanup. A sustained workload/resource trend is still required before attributing a historical RAM peak to a specific defect or claiming long-term growth has stopped.
