// Makes the bot and the three ended calls `call-conversation.yaml` opens, on the host the phone is paired with
// (HOST_PORT and HOST_TOKEN, passed with `-e`), or (`CLEANUP`, when the flow completes) deletes that bot again.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}
const id = () => `maestro-${Date.now()}-${Math.random().toString(36).slice(2, 10)}`

/** A call the host has ended, with what was said on it, as its line in the chat. */
function endedCall(botId, seconds, said) {
  const callId = id()
  api('startCall', { botId, callId, mode: 'device' })
  const at = Date.now()
  api('endCall', { botId, callId, seconds, lines: said.map(([from, text], i) => ({ from, text, at: at + i * 1000 })) })
}

if (typeof CLEANUP !== 'undefined') {
  if (output.bot) api('deleteBot', { botId: output.bot.id })
} else {
  const name = `Call check ${Math.random().toString(36).slice(2, 6)}`
  const bot = api('createBot', {
    name, avatarColor: 'black', avatarShape: 'blob', backend: 'claude', cwd: '/tmp', connectors: [], skills: [],
  }).bot
  endedCall(bot.id, 3, [])
  endedCall(bot.id, 16, [
    ['user', 'Can you check the build?'],
    ['voice', 'Sure, it passed a minute ago.'],
    ['voice', 'Two tests were skipped.'],
    ['user', 'Thanks, that is all.'],
    ['voice', 'You are welcome.'],
  ])
  const long = []
  for (let i = 1; i <= 20; i++) long.push(['user', `Question ${i} about the release?`], ['voice', `Answer ${i}: it goes out once the review is green.`])
  endedCall(bot.id, 185, long)
  output.bot = { name, id: bot.id }
}
