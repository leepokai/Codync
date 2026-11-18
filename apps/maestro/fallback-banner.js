// Makes the bot `fallback-banner.yaml` opens, on the host the phone is paired with (HOST_PORT and HOST_TOKEN, passed
// with `-e`): on Kimi Code, played by host/tests/fixtures/relay_agent.py (FIXTURE_AGENT, its full path), which asks to
// sign in, with Codex (the same script) as its fallback; one message puts it on Codex while Kimi Code rests signed out.
// `CLEANUP`, when the flow completes, deletes the bot again.
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
  const name = `Fallback check ${Math.random().toString(36).slice(2, 6)}`
  const agent = (as) => `python3 -X utf8 -u '${FIXTURE_AGENT}' ${as}`
  const bot = api('createBot', {
    // The agent runs in the bot's workspace, so the file that signs it out is made there (in a shell of its own: the
    // host execs the command, which would leave only `touch`).
    name, backend: 'kimi', command: `sh -c "touch primary.signedout && exec ${agent('primary')}"`, connectors: [], skills: [], notify: false,
    fallbacks: [{ backend: 'codex', command: agent('standin'), model: null, modelOptions: {} }],
  }).bot
  api('send', { botId: bot.id, text: 'Hello', clientNonce: `fallback-${bot.id}` })
  let kept
  for (let tries = 0; tries < 60 && !(kept && kept.agentStep && kept.resting && kept.resting.length); tries++) {
    const until = Date.now() + 500
    while (Date.now() < until) {} // ponytail: the script has no sleep; a busy wait of at most 30 s
    kept = api('updateBot', { id: bot.id }).bot
  }
  if (!kept.agentStep) throw new Error('the bot never moved to its fallback')
  if (kept.resting[0].reason !== 'signedOut') throw new Error(`Kimi Code rests as ${kept.resting[0].reason}, not signedOut`)
  output.bot = { name, id: bot.id }
}
