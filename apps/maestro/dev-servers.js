// Makes the bot `dev-servers.yaml` opens, on the host the phone is paired with (HOST_PORT and HOST_TOKEN, passed with
// `-e`), with a server listening only on the computer, one on all interfaces and one whose command fails. The tool's call
// comes through the host's API, so no agent or model runs. `CLEANUP`, when the flow completes, deletes the bot again.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}

if (typeof CLEANUP !== 'undefined') {
  if (output.bot) api('deleteBot', { botId: output.bot.id })
} else {
  const name = `Dev server check ${Math.random().toString(36).slice(2, 6)}`
  const bot = api('createBot', {
    name, backend: 'custom', command: 'unused', connectors: [], skills: [], permission: 'auto', notify: false,
  }).bot
  const start = (server, command) =>
    api('chatCall', { botId: bot.id, name: 'start_dev_server', arguments: { name: server, command } })
  // Below where the system hands out ports, so it isn't taken for one of its own.
  const port = 20000 + Math.floor(Math.random() * 10000)
  start('docs', 'echo no such script; exit 3')
  start('web', `python3 -m http.server ${port} --bind 127.0.0.1`)
  // From an empty folder: on all interfaces, it would otherwise share the bot's with the network.
  start('shared', `python3 -m http.server ${port + 1} --directory "$(mktemp -d)"`)
  output.bot = { name, id: bot.id }
  output.port = String(port)
  output.sharedPort = String(port + 1)
}
