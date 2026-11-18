// Makes the chat `reply-part.yaml` replies in, on the host the phone is paired with (HOST_PORT and HOST_TOKEN, passed
// with `-e`). Its agent is host/tests/fixtures/mode_agent.py (MODE_AGENT, its full path), which answers each message
// with `mode agent · ` and what it was sent. `CLEANUP`, when the flow completes, deletes the bot again.
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
} else if (typeof QUOTE !== 'undefined') {
  // A long message pushes the quoted one out of sight, then a late answer quotes it (`reply_to`), as a background
  // result sends it.
  const text = Array.from({ length: 60 }, (_, i) => `Line ${i}.`).join('\n\n')
  api('chatCall', { botId: output.bot.id, name: 'send_message', arguments: { text } })
  api('chatCall', { botId: output.bot.id, name: 'send_message', arguments: { text: 'About that one.', reply_to: output.asked } })
} else {
  const name = `Reply check ${Math.random().toString(36).slice(2, 6)}`
  const bot = api('createBot', {
    name, backend: 'custom', command: `python3 -X utf8 -u '${MODE_AGENT}'`, connectors: [], skills: [], notify: false,
  }).bot
  output.bot = { name, id: bot.id }
  // The first message of a session carries the bot's whole profile, which the agent would answer with: the flow replies
  // to the answer to the second one, which is only `mode agent · alpha beta gamma`.
  let answered = false
  let asked
  for (const text of ['hello', 'alpha beta gamma']) {
    asked = api('send', { botId: bot.id, text }).entry
    answered = false
    for (let tries = 0; tries < 100 && !answered; tries++) {
      const until = Date.now() + 100
      while (Date.now() < until) {} // ponytail: the script has no sleep; a busy wait of at most 10 s
      answered = api('history', { botId: bot.id, limit: 5 }).entries.some((e) => e.data.final && e.data.text.endsWith(text))
    }
    if (!answered) break
  }
  if (!answered) throw new Error('the bot never answered')
  output.asked = asked.id
}
