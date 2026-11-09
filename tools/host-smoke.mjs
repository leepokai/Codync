// Starts a real host on a free port with a temporary data folder, then checks the API the apps depend on.
// Usage: node tools/host-smoke.mjs [path-to-codync-host]
import { spawn } from 'node:child_process'
import { mkdtemp, readFile, rm } from 'node:fs/promises'
import { existsSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { setTimeout as sleep } from 'node:timers/promises'

const binary = process.argv[2] ?? (process.platform === 'win32'
  ? 'host/target/debug/codync-host.exe'
  : 'host/target/debug/codync-host')
if (!existsSync(binary)) {
  console.error(`No host binary at ${binary}; build it first (cd host && cargo build).`)
  process.exit(2)
}

const home = await mkdtemp(join(tmpdir(), 'codync-smoke-'))
const port = 19400 + Math.floor(Math.random() * 400)
const host = spawn(binary, ['serve', '--port', String(port), '--bind', '127.0.0.1'], {
  env: { ...process.env, CODYNC_HOME: home },
  stdio: ['ignore', 'ignore', 'ignore'],
})

let token = null
let up = false
for (let i = 0; i < 120 && !up; i += 1) {
  await sleep(250)
  try {
    const health = await fetch(`http://127.0.0.1:${port}/health`)
    up = health.ok
  } catch {
    // still starting
  }
}
if (!up) {
  host.kill()
  await rm(home, { recursive: true, force: true })
  console.error('The host did not answer /health in 30 s.')
  process.exit(1)
}
for (let i = 0; i < 20 && token === null; i += 1) {
  try {
    token = (await readFile(join(home, 'token'), 'utf8')).trim()
  } catch {
    await sleep(100)
  }
}

const call = async (method, body = {}) => {
  const res = await fetch(`http://127.0.0.1:${port}/api/${method}`, {
    method: 'POST',
    headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
    body: JSON.stringify(body),
  })
  const answer = await res.json().catch(() => ({}))
  if (!res.ok) throw new Error(`${method}: HTTP ${res.status} ${JSON.stringify(answer)}`)
  return answer
}

const checks = []
const check = (name, ok, detail = '') => checks.push({ name, ok, detail })

try {
  const health = await call('hostHealth')
  check('hostHealth answers with a level', typeof health.level === 'string', health.level)
  const diagnostics = await call('diagnostics')
  check('diagnostics has a version and log', typeof diagnostics.hostVersion === 'string' && Array.isArray(diagnostics.log), diagnostics.hostVersion)
  check('diagnostics redacts long secrets', diagnostics.log.every((line) => !/Bearer [A-Za-z0-9._-]{20,}/.test(line)))
  const audit = await call('auditLog')
  check('auditLog answers a list', Array.isArray(audit.lines))
  const enterprise = await call('enterpriseStatus')
  check('enterpriseStatus says it is unset', enterprise.configured === false)
  const unauthorized = await fetch(`http://127.0.0.1:${port}/api/hostHealth`, { method: 'POST' })
  check('the API refuses a missing token', unauthorized.status === 401, String(unauthorized.status))
} catch (error) {
  check('the checks ran', false, error instanceof Error ? error.message : String(error))
} finally {
  host.kill()
  await sleep(1500)
  await rm(home, { recursive: true, force: true })
}

let failed = 0
for (const { name, ok, detail } of checks) {
  if (!ok) failed += 1
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${name}${detail ? ` (${detail})` : ''}`)
}
console.log(failed === 0 ? `\n${checks.length} checks passed.` : `\n${failed} of ${checks.length} checks failed.`)
process.exit(failed === 0 ? 0 : 1)
