// The computer's side of `dictation-send.yaml`: an OpenAI-compatible server that answers every recording with the same
// transcript, saved on the host the phones are paired with as its OpenAI engine (with a made-up key), so nothing real
// is transcribed or billed. Ctrl-C clears the key and the server again.
//   node apps/maestro/dictation-send-server.mjs <sandbox port> "$(cat ~/.codync-dev-<sandbox name>/token)"
import { createServer } from 'node:http'

const [port, token] = process.argv.slice(2)
const api = async (method, body) => {
  const res = await fetch(`http://127.0.0.1:${port}/api/${method}`, {
    method: 'POST',
    headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`${method}: ${res.status} ${await res.text()}`)
}
const compatible = createServer((req, res) => {
  res.setHeader('Content-Type', 'application/json')
  req.resume()
  req.on('end', () => res.end(req.url?.includes('/audio/transcriptions') ? '{"text":"and the follow-up"}' : '{"data":[{"id":"whisper-1"}]}'))
})
compatible.listen(0, '127.0.0.1', async () => {
  await api('setVoiceServer', { provider: 'openai', url: `http://127.0.0.1:${compatible.address().port}` })
  await api('setVoiceKey', { provider: 'openai', key: 'sk-e2e-local' })
  console.log('ready')
})
for (const signal of ['SIGINT', 'SIGTERM']) {
  process.on(signal, async () => {
    await api('setVoiceKey', { provider: 'openai', key: '' }).catch(() => {})
    await api('setVoiceServer', { provider: 'openai', url: '' }).catch(() => {})
    process.exit(0)
  })
}
