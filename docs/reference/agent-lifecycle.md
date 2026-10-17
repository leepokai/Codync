# Agent process ownership

The host owns each ACP adapter and its descendants for the adapter's lifetime.
On Unix, the adapter starts in its own process group. A monitor observes leader
exit with `waitid(WNOWAIT)` and kills the group before reaping the leader, so
cleanup cannot target a reused process ID. Dropping the owner also terminates
the group. Descendants that deliberately create a separate process group remain
outside this Unix group; idle retirement checks for those background tools.
On Windows, the shell starts suspended, joins a job with
`KILL_ON_JOB_CLOSE`, then resumes. Closing the job releases its descendants.

Intentional shutdown first closes ACP stdin and retains process ownership during
the existing ten-second grace period, allowing CLI login refreshes to finish.
After that deadline the host terminates remaining descendants and reaps the
leader. Unexpected leader exit and actor unwinding release the owned tree.

Abandoning an ACP answer removes its pending request. An interrupted session
allocation is marked uncertain until its identity is confirmed. The next turn
restarts a resumable adapter before allocating again; an adapter without
`loadSession` keeps confirmed conversations and requires an explicit reset
before another uncertain allocation. Fast mode and idle session closure use a
separate request path and never mark an allocation uncertain.

A session snapshot persists the first-prompt profile and thread introduction.
The host records a new or forked identity before model setup, so setup failure
can retry the same conversation. Initial instructions clear only after the
prompt is written; they survive another lane running or a host restart.

The existing idle policy remains: sessions unused for thirty minutes may close
when supported, and an entirely idle resumable adapter may stop. A queue,
approval, active turn or detached background descendant holds it open. Agents
without `loadSession` remain live. There is no per-process session cap.
A failed saved-session load retains the existing fresh-session fallback with a
divider notice, for both main chats and threads. Fork-only adapters retain their
existing thread-fork support.
