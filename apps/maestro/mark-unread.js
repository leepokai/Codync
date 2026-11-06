// Makes the bot `mark-unread.yaml` reads, with its platform's own number of replies, on the host the phone is paired with (HOST_PORT and HOST_TOKEN,
// passed with `-e`). Its agent is host/tests/fixtures/steer_agent.py (STEER_AGENT, its full path). `MARK` instead puts it
// back to unread as another device would, and `CLEANUP`, when the flow completes, deletes the bot again.
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
} else if (typeof MARK !== 'undefined') {
  const answer = api('markUnread', { botId: output.bot.id })
  if (!answer.bot.keptUnread) throw new Error('not held: ' + JSON.stringify(answer.bot))
} else {
  const name = `Unread ${maestro.platform} ${Math.random().toString(36).slice(2, 6)}`
  const bot = api('createBot', {
    name, backend: 'custom', command: `python3 -X utf8 -u '${STEER_AGENT}' plain 0.3`, connectors: [], skills: [], notify: false,
  }).bot
  const replies = maestro.platform === 'ios' ? 2 : 1
  for (let i = 1; i <= replies; i++) api('send', { botId: bot.id, text: `message ${i}` })
  output.bot = { name, id: bot.id }
  output.unread = `${replies}( unread)?`
}
