// Makes the bots `own-folder.yaml` opens, on the host the phone is paired with (HOST_PORT and HOST_TOKEN, passed with
// `-e`): one with no project, and one with PROJECT_DIR, a git repository, as its project, once the host says it is a
// repository. Their agent is host/tests/fixtures/instructions_agent.py (FIXTURE_AGENT, its full path, as
// for `compaction.yaml`). `DEFAULT_IS` checks the project bot's default project; `CLEANUP`, when the flow completes, deletes them again.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}

/** A new bot named `name`, with `project` as its project when given, once the host says that one is a repository. */
const make = (name, project) => {
  const id = api('createBot', {
    name, backend: 'custom', command: `python3 -X utf8 -u '${FIXTURE_AGENT}'`, projects: project ? [project] : [], connectors: [], skills: [], notify: false,
  }).bot.id
  if (!project) return { name, id }
  // An update that changes nothing answers with the bot as the host has it.
  for (let tries = 0; tries < 50; tries++) {
    if (api('updateBot', { id }).bot.gitRepo) return { name, id }
    const until = Date.now() + 200
    while (Date.now() < until) {} // ponytail: the script has no sleep; a busy wait of at most 10 s
  }
  throw new Error(`${name}'s project never became a repository`)
}

if (typeof DEFAULT_IS !== 'undefined') {
  // The host has DEFAULT_IS as the project bot's default, once the app has saved it.
  let first
  for (let tries = 0; tries < 50; tries++) {
    first = api('updateBot', { id: output.project.id }).bot.projects[0]
    if (first === DEFAULT_IS) break
    const until = Date.now() + 200
    while (Date.now() < until) {} // ponytail: the script has no sleep; a busy wait of at most 10 s
  }
  if (first !== DEFAULT_IS) throw new Error(`the default project is ${first}, not ${DEFAULT_IS}`)
} else if (typeof CLEANUP !== 'undefined') {
  for (const bot of [output.bot, output.project]) if (bot) api('deleteBot', { botId: bot.id })
} else {
  const tag = Math.random().toString(36).slice(2, 6)
  output.bot = make(`Own folder check ${tag}`)
  const project = make(`Project check ${tag}`, PROJECT_DIR)
  project.folder = PROJECT_DIR.split('/').filter(Boolean).pop()
  output.added = ADD_DIR.split('/').filter(Boolean).pop()
  output.project = project
}
