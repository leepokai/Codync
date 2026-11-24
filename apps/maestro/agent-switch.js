// Makes the bot `agent-switch.yaml` opens, on the host the phone is paired with (HOST_PORT and HOST_TOKEN, passed with
// `-e`): on a custom command played by host/tests/fixtures/relay_agent.py (FIXTURE_AGENT, its full path), with one
// exchange, then moved by the user to Codex (the same script), so the chat owes Codex a recap. `STEP: send` sends the
// next message, which carries it, and names a line of what was carried; `CLEANUP` deletes the bot.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}
const agent = (as) => `python3 -X utf8 -u '${FIXTURE_AGENT}' ${as}`
const switchLine = () => api('history', { botId: output.bot.id }).entries.find((e) => e.data && e.data.agentSwitch && e.data.agentSwitch.manual)
// Waits up to 30 s for `done` to return something.
const until = (what, done) => {
  for (let tries = 0; tries < 60; tries++) {
    const got = done()
    if (got) return got
    const stop = Date.now() + 500
    while (Date.now() < stop) {} // ponytail: the script has no sleep; a busy wait of at most 30 s
  }
  throw new Error(what)
}

if (typeof CLEANUP !== 'undefined') {
  if (output.bot) api('deleteBot', { botId: output.bot.id })
} else if (typeof STEP === 'undefined') {
  const name = `Switch check ${Math.random().toString(36).slice(2, 6)}`
  const bot = api('createBot', { name, backend: 'custom', command: agent('primary'), connectors: [], skills: [], notify: false }).bot
  output.bot = { name, id: bot.id }
  api('send', { botId: bot.id, text: 'Fix the login tests', clientNonce: `switch-${bot.id}-1` })
  until('the bot never answered', () => api('updateBot', { id: bot.id }).bot.status === 'idle'
    && api('history', { botId: bot.id }).entries.some((e) => e.kind === 'agent'))
  api('updateBot', { id: bot.id, backend: 'codex', command: agent('standin') })
  until('no pending switch line', () => { const l = switchLine(); return l && l.data.agentSwitch.state === 'pending' })
} else {
  api('send', { botId: output.bot.id, text: 'And the signup tests', clientNonce: `switch-${output.bot.id}-2` })
  const line = until('the switch line never read handed off', () => { const l = switchLine(); return l && l.data.agentSwitch.state === 'handedOff' && l })
  // A line only the unfolded recap shows (the chat's own messages are on screen too), as a regex.
  const carried = line.data.agentSwitch.carried.split('\n').map((l) => l.trim()).find((l) => l.length > 12 && !/login tests|signup tests/.test(l))
  output.carried = '.*' + carried.slice(0, 30).replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + '.*'
}
