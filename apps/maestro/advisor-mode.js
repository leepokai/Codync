// Makes the bot `advisor-mode.yaml` opens, on the host the phone is paired with (HOST_PORT and HOST_TOKEN, passed with
// `-e`): on Kimi Code, with Codex as its fallback. `STEP=on` checks the threads start on the bot's own agent and
// fallbacks; `STEP=picked`, the threads on Claude Code while the chat stays on Kimi Code; `STEP=added`, one more threads'
// fallback while the chat's chain stays as it was; `STEP=off`, no threads' agent or chain. `CLEANUP`, when the flow completes, deletes the bot.
const api = (method, body) => {
  const res = http.post(`http://127.0.0.1:${HOST_PORT}/api/${method}`, {
    headers: { Authorization: `Bearer ${HOST_TOKEN}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (res.status !== 200) throw new Error(`${method}: ${res.status} ${res.body}`)
  return json(res.body)
}
const chain = (steps) => (steps || []).map((f) => f.backend).join(',')

if (typeof CLEANUP !== 'undefined') {
  if (output.bot) api('deleteBot', { botId: output.bot.id })
} else if (typeof STEP !== 'undefined') {
  const bot = api('updateBot', { id: output.bot.id }).bot
  const seen = `threads on ${bot.threadAgent ? bot.threadAgent.backend : 'none'} [${chain(bot.threadFallbacks)}], chat [${chain(bot.fallbacks)}]`
  const expected = {
    on: /^threads on kimi \[codex\], chat \[codex\]$/,
    picked: /^threads on claude \[codex\], chat \[codex\]$/,
    added: /^threads on claude \[codex,[^\]]+\], chat \[codex\]$/,
    off: /^threads on none \[\], chat \[codex\]$/,
  }[STEP]
  if (!expected.test(seen)) throw new Error(`${seen}, not ${expected}`)
  const steps = (bot.threadFallbacks || []).length
  if (STEP === 'added' && steps !== output.threadSteps + 1) throw new Error(`${seen}: not one step more than ${output.threadSteps}`)
  output.threadSteps = steps
} else {
  const name = `Advisor ${Math.random().toString(36).slice(2, 6)}`
  const bot = api('createBot', {
    name, backend: 'kimi', connectors: [], skills: [], notify: false,
    fallbacks: [{ backend: 'codex', model: null, modelOptions: {} }],
  }).bot
  output.bot = { name, id: bot.id }
}
