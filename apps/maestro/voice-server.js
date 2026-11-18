// Helps `voice-server.yaml`, on the host the phone is paired with (HOST_PORT and HOST_TOKEN, passed with `-e`): makes
// the bot whose chat the flow dictates in; `STEP: server` saves an OpenAI-compatible server on the computer, as its own
// Settings do; `CLEANUP` (when the flow completes) deletes the bot and clears the server. Phones in one run share that
// host and its one server, so each saves an address of its own and clears only its own: the other phone's stays for it.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}
const server = `https://${maestro.platform}.speech.example.com`
const saved = () => api('voiceStatus', {}).providers.find((p) => p.provider === 'openai').server

if (typeof CLEANUP !== 'undefined') {
  if (output.bot) api('deleteBot', { botId: output.bot.id })
  if (saved() === server) api('setVoiceServer', { provider: 'openai', url: '' })
} else if (typeof STEP === 'undefined') {
  const name = `Voice server check ${Math.random().toString(36).slice(2, 6)}`
  const bot = api('createBot', {
    name, avatarColor: 'black', avatarShape: 'blob', backend: 'claude', cwd: '/tmp', connectors: [], skills: [],
  }).bot
  output.bot = { name, id: bot.id }
} else if (STEP === 'server') {
  api('setVoiceServer', { provider: 'openai', url: server })
}
