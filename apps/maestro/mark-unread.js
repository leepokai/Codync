// Makes the bot `mark-unread.yaml` reads, with two replies, on the host the phone is paired with (HOST_PORT and HOST_TOKEN,
// passed with `-e`). Its agent is host/tests/fixtures/steer_agent.py (STEER_AGENT, its full path). `MARK` instead puts it
// back to unread as another device would; `REPLY_WHILE_MARKED` does that with a reply on its way, and waits for it;
// `AWAIT_REPLY` waits for the reply to what the phone sent after `MARK`; `SEND` sends two messages
// and waits for their replies; and `CLEANUP`, when the flow completes, deletes the bot again.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}

// Two quick messages make two unread replies: the host reads the chat at each send and joins what queues behind a running turn.
const REPLIES = 2

const repliesSoFar = () =>
  api('history', { botId: output.bot.id, limit: 500 }).entries.filter((e) => e.kind === 'agent' && e.data.final).length

const repliesLand = (count) => {
  const deadline = Date.now() + 20000
  while (repliesSoFar() < count) {
    if (Date.now() > deadline) throw new Error(`no reply ${count} yet`)
  }
}

const send = (text) => api('send', { botId: output.bot.id, text })

const mark = () => {
  const answer = api('markUnread', { botId: output.bot.id })
  if (!answer.bot.keptUnread) throw new Error('not held: ' + JSON.stringify(answer.bot))
}

if (typeof CLEANUP !== 'undefined') {
  if (output.bot) api('deleteBot', { botId: output.bot.id })
} else if (typeof MARK !== 'undefined') {
  output.repliesAtMark = repliesSoFar()
  mark()
} else if (typeof AWAIT_REPLY !== 'undefined') {
  repliesLand(output.repliesAtMark + 1)
} else if (typeof REPLY_WHILE_MARKED !== 'undefined') {
  const had = repliesSoFar()
  send('one more')
  mark()
  repliesLand(had + 1)
} else if (typeof SEND !== 'undefined') {
  const had = repliesSoFar()
  for (let i = 1; i <= REPLIES; i++) send(`on screen ${i}`)
  repliesLand(had + REPLIES)
} else {
  const name = `Unread ${maestro.platform} ${Math.random().toString(36).slice(2, 6)}`
  const bot = api('createBot', {
    name, backend: 'custom', command: `python3 -X utf8 -u '${STEER_AGENT}' plain 0.3`, connectors: [], skills: [], notify: false,
  }).bot
  output.bot = { name, id: bot.id }
  for (let i = 1; i <= REPLIES; i++) send(`message ${i}`)
  output.unread = `${REPLIES}( unread)?`
  output.onePlus = `${REPLIES + 1}( unread)?`
  output.twoPlus = `${REPLIES + 2}( unread)?`
}
