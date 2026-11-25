// Makes the bot `shared-folders.yaml` opens, on the host the phone is paired with (HOST_PORT and HOST_TOKEN, passed
// with `-e`), with PROJECT_DIR, a git repository, as its project, once the host says it is a repository. Its agent is
// host/tests/fixtures/instructions_agent.py (FIXTURE_AGENT). `SAVED` checks the project's shared folders on the host
// (comma-separated, "" for none); `CLEANUP`, when the flow completes, deletes the bot again.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}

/** Waits until `check()` holds, for at most 10 s. */
const until = (check, what) => {
  for (let tries = 0; tries < 50; tries++) {
    if (check()) return
    const end = Date.now() + 200
    while (Date.now() < end) {} // ponytail: the script has no sleep; a busy wait of at most 10 s
  }
  throw new Error(what)
}

if (typeof SAVED !== 'undefined') {
  // An update that changes nothing answers with the bot as the host has it.
  const shared = () => ((api('updateBot', { id: output.bot.id }).bot.sharedFolders || {})[PROJECT_DIR] || []).join(',')
  until(() => shared() === SAVED, `the shared folders are "${shared()}", not "${SAVED}"`)
} else if (typeof CLEANUP !== 'undefined') {
  if (output.bot) api('deleteBot', { botId: output.bot.id })
} else {
  const name = `Shared folders check ${Math.random().toString(36).slice(2, 6)}`
  const id = api('createBot', {
    name, backend: 'custom', command: `python3 -X utf8 -u '${FIXTURE_AGENT}'`, projects: [PROJECT_DIR], connectors: [], skills: [], notify: false,
  }).bot.id
  until(() => api('updateBot', { id }).bot.gitRepo, `${name}'s project never became a repository`)
  output.bot = { name, id }
}
