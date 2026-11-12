// One-off: lists the connector catalogue the host serves and checks the bridge fields that decide
// how each entry installs. Start a debug host first (host/target/debug/codync-host.exe).
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
const exa = items.find((a) => a.id === 'exa-authenticated')
const google = items.find((a) => a.id === 'google-drive')
const notion = items.find((a) => a.id === 'notion')
const routines = items.find((a) => a.id === 'routines')
console.log('apps:', items.length)
console.log('featured:', items.filter((a) => a.featured).map((a) => a.id).join(', '))
console.log('exa key field:', exa && exa.fields.map((f) => `${f.header}:${f.template}`).join(', '))
console.log('google:', google && { setup: google.setup, url: google.server.url, fields: google.fields.map((f) => f.id) })
console.log('notion:', notion && { setup: notion.setup, fields: notion.fields.length, url: notion.server.url })
console.log('routines:', routines && { setup: routines.setup, builtin: routines.builtin, installed: routines.installed })

const failures = []
if (items.length !== 9) failures.push(`${items.length} apps, expected the 9 CopilotKit connectors`)
if (!items.some((a) => a.featured)) failures.push('no featured pick')
if (items.some((a) => !a.name || !a.server.name || !a.server.transport)) failures.push('app without a server')
if (items.some((a) => !a.builtin && !a.server.url)) failures.push('remote app without a URL')
if (!exa || exa.setup !== 'key' || exa.fields[0]?.header !== 'x-api-key' || exa.fields[0]?.template !== '{apiKey}') {
  failures.push('exa key field in its own header, unwrapped')
}
if (!google || google.setup !== 'link' || google.fields.length !== 2) failures.push('google asks for its own OAuth client')
if (!notion || notion.setup !== 'link' || notion.fields.length !== 0) failures.push('notion registers itself')
if (!routines || routines.builtin !== true || routines.installed !== true) failures.push('routines is always on')

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
