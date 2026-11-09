// Makes the bot `compaction.yaml` opens, on the host the phone is paired with (HOST_PORT and HOST_TOKEN, passed with
// `-e`), and has it reply once. Its agent is host/tests/fixtures/instructions_agent.py (FIXTURE_AGENT, its full path),
// which answers as Claude's adapter does and reports the window its message names, so the bot offers sizes. `STORED`
// checks the host keeps that setting; `CLEANUP`, when the flow completes, deletes the bot again.
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
} else if (typeof STORED !== 'undefined') {
  // The apps save 600 ms after the last change. An update that changes nothing answers with the bot as the host keeps it.
  let kept
  for (let tries = 0; tries < 10 && String(kept) !== STORED; tries++) {
    const until = Date.now() + 300
    while (Date.now() < until) {} // ponytail: the script has no sleep; a busy wait of at most 3 s
    kept = api('updateBot', { id: output.bot.id }).bot.compaction
  }
  if (String(kept) !== STORED) throw new Error(`the host keeps ${kept}, not ${STORED}`)
} else {
  const name = `Summarize check ${Math.random().toString(36).slice(2, 6)}`
  const bot = api('createBot', {
    name, backend: 'custom', command: `python3 -X utf8 -u '${FIXTURE_AGENT}' claude`, connectors: [], skills: [], notify: false,
  }).bot
  api('send', { botId: bot.id, text: 'USAGE 10000 SIZE 1000000', clientNonce: `compaction-${bot.id}` })
  output.bot = { name, id: bot.id }
}
