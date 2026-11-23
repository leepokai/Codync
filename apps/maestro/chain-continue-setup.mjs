// The computer's side of `chain-continue.yaml`: for each phone, two bots on host/tests/fixtures/team_agent.py hand a
// chain back and forth (each folder's handoff.json names the other) until the handoff limit stops it, which leaves a
// Continue notice in the first bot's chat. The agent reads handoff.json from its folder, which a flow's script can't
// write, so this runs before the Maestro run and stays up; Ctrl-C deletes the bots and their folders.
//   node apps/maestro/chain-continue-setup.mjs <sandbox port> "$(cat ~/.codync-dev-<sandbox name>/token)" &
// Once it prints the `-e CHAIN_IOS=… -e CHAIN_ANDROID=…` line, pass that to the Maestro run.
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

const [port, token] = process.argv.slice(2)
const agent = resolve(import.meta.dirname, '../../host/tests/fixtures/team_agent.py')
const api = async (method, body) => {
  const res = await fetch(`http://127.0.0.1:${port}/api/${method}`, {
    method: 'POST',
    headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`${method}: ${res.status} ${await res.text()}`)
  return res.json()
}
const folder = mkdtempSync(join(tmpdir(), 'chain-continue-'))
const made = []
const bot = async (name) => {
  const cwd = join(folder, name)
  mkdirSync(cwd)
  const { bot } = await api('createBot', {
    name, backend: 'custom', command: `python3 -X utf8 -u '${agent}'`, cwd, connectors: [], skills: [], notify: false,
  })
  made.push(bot.id)
  return { id: bot.id, cwd }
}
const cleanUp = async () => {
  for (const botId of made) {
    await api('stop', { botId }).catch(() => {})
    await api('deleteBot', { botId }).catch(() => {})
  }
  rmSync(folder, { recursive: true, force: true })
}
for (const signal of ['SIGINT', 'SIGTERM']) process.on(signal, () => cleanUp().then(() => process.exit(0)))

// The flow opens "Chain ios" / "Chain android" by the phone's platform.
const ids = {}
for (const platform of ['ios', 'android']) {
  const first = await bot(`Chain ${platform}`)
  const second = await bot(`Chain ${platform} peer`)
  writeFileSync(join(first.cwd, 'handoff.json'), JSON.stringify({ target: second.id }))
  writeFileSync(join(second.cwd, 'handoff.json'), JSON.stringify({ target: first.id }))
  ids[platform] = first.id
  await api('send', { botId: first.id, text: 'CHAIN' })
  const deadline = Date.now() + 60_000
  while (!(await api('history', { botId: first.id })).entries.some((e) => e.data.continueChain === true)) {
    if (Date.now() > deadline) throw new Error(`no Continue notice for ${platform}`)
    await new Promise((r) => setTimeout(r, 500))
  }
}
console.log(`-e CHAIN_IOS=${ids.ios} -e CHAIN_ANDROID=${ids.android}`)
