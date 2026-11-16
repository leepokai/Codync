// Makes the bot `fallback-edit.yaml` opens, on the host the phone is paired with (HOST_PORT and HOST_TOKEN, passed with
// `-e`): on Kimi Code, with Codex on the model "e2e-model" and Gemini CLI as its fallbacks. `STEP=saved` checks the
// chain after the edit: Codex back on its default model, still first; `STEP=added`, a third step after them. `CLEANUP`,
// when the flow completes, deletes the bot.
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
} else if (typeof STEP !== 'undefined') {
  const chain = api('updateBot', { id: output.bot.id }).bot.fallbacks.map((f) => `${f.backend}:${f.model || ''}`).join(',')
  const expected = STEP === 'added' ? /^codex:,gemini:,[^,:]+:$/ : /^codex:,gemini:$/
  if (!expected.test(chain)) throw new Error(`the fallbacks are ${chain}, not ${expected}`)
} else {
  const name = `Fallback edit ${Math.random().toString(36).slice(2, 6)}`
  const bot = api('createBot', {
    name, backend: 'kimi', connectors: [], skills: [], notify: false,
    fallbacks: [{ backend: 'codex', model: 'e2e-model', modelOptions: {} }, { backend: 'gemini', model: null, modelOptions: {} }],
  }).bot
  output.bot = { name, id: bot.id }
}
