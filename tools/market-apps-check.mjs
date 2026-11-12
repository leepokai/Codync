// One-off: counts the marketplace apps the host serves, and checks a sample entry's bridge fields.
import { spawn } from 'node:child_process'
import { mkdtemp, readFile, rm } from 'node:fs/promises'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { setTimeout as sleep } from 'node:timers/promises'

const home = await mkdtemp(join(tmpdir(), 'codync-apps-'))
const port = 19800 + Math.floor(Math.random() * 150)
const host = spawn('host/target/debug/codync-host.exe', ['serve', '--port', String(port), '--bind', '127.0.0.1'], {
  env: { ...process.env, CODYNC_HOME: home },
  stdio: ['ignore', 'ignore', 'ignore'],
})

let token = null
for (let i = 0; i < 120; i += 1) {
  await sleep(250)
  try {
    if ((await fetch(`http://127.0.0.1:${port}/health`)).ok) {
      token = (await readFile(join(home, 'token'), 'utf8')).trim()
      break
    }
  } catch {}
}
if (!token) {
  host.kill()
  console.error('host did not start')
  process.exit(1)
}

const call = async (method, body = {}) => {
  const res = await fetch(`http://127.0.0.1:${port}/api/${method}`, {
    method: 'POST',
    headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  if (!res.ok) throw new Error(`${method} HTTP ${res.status}`)
  return res.json()
}

const { items } = await call('marketApps')
const figma = items.find((a) => a.id === 'figma')
const airtable = items.find((a) => a.id === 'airtable')
console.log('apps:', items.length)
console.log('featured:', items.filter((a) => a.featured).map((a) => a.id).join(', '))
console.log('first six:', items.slice(0, 6).map((a) => a.id).join(', '))
console.log('figma:', figma && { setup: figma.setup, transport: figma.server.transport, url: figma.server.url, steps: figma.steps.length })
console.log('airtable:', airtable && { setup: airtable.setup, transport: airtable.server.transport, command: airtable.server.command, fields: airtable.fields.length })

const failures = []
if (items.length < 240) failures.push(`only ${items.length} apps`)
if (!figma || figma.setup !== 'local' || figma.steps.length === 0) failures.push('figma local steps')
if (!airtable || airtable.setup !== 'key' || airtable.fields.length === 0) failures.push('airtable key fields')
if (items.some((a) => !a.server.name || !a.server.transport)) failures.push('app without server')

host.kill()
// The host releases its SQLite file a moment after the process ends.
for (let i = 0; i < 20; i += 1) {
  try {
    await rm(home, { recursive: true, force: true })
    break
  } catch {
    await sleep(250)
  }
}
if (failures.length > 0) {
  console.error('failed:', failures.join('; '))
  process.exit(1)
}
console.log('market apps ok')
