// Starts the thread without a bot `thread-own-folder.yaml` opens, on the host the phone is paired with (HOST_PORT and
// HOST_TOKEN, passed with `-e`): no folder picked, so it works in a folder of its own the host makes for it.
// `CLEANUP`, when the flow completes, drops it again (its folder goes with it).
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}

/** Waits until `check()` holds, for at most 90 s. */
const until = (check, what) => {
  for (let tries = 0; tries < 450; tries++) {
    if (check()) return
    const end = Date.now() + 200
    while (Date.now() < end) {} // ponytail: the script has no sleep; a busy wait of at most 90 s
  }
  throw new Error(what)
}

if (typeof ENDED !== 'undefined') {
  // Its agent answers once and the thread ends: then its chat opens the same on both phones.
  const status = () => (api('updateBot', { id: output.thread.botId }).bot.work || []).find((t) => t.id === output.thread.taskId)?.status
  until(() => ['done', 'failed', 'stopped'].includes(status()), `the thread is still ${status()}`)
} else if (typeof CLEANUP !== 'undefined') {
  // One still working can't be dropped yet: it ends on its own, and the sandbox goes with the worktree.
  if (output.thread) try { api('dropWork', { botId: output.thread.botId, taskId: output.thread.taskId }) } catch (e) {}
} else {
  // Each phone's shard starts its own, so each has a title of its own.
  const title = `Own folder check ${Math.random().toString(36).slice(2, 6)}`
  const started = api('startThread', { text: title, setup: { backend: 'claude' } })
  output.thread = { title, botId: started.botId, taskId: started.taskId }
}
