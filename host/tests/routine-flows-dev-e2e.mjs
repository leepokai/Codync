// Manual end-to-end run of routine flows against the `dev` host: port 19223 and a data directory
// of its own, never the production host on 19222. Not a `.test.mjs`: it needs a built host.
//
//   node tests/routine-flows-dev-e2e.mjs [path/to/codync-host] [port]
//
// Three bots (the deterministic team agent), one routine, A -> B -> C, a test of the flow, a
// failure in B that leaves C skipped, and a host restart in the middle of a run.
import assert from 'node:assert/strict'
import net from 'node:net'
import { spawn } from 'node:child_process'
import { mkdtempSync, mkdirSync, readFileSync, rmSync, existsSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, resolve } from 'node:path'

const exe = resolve(process.argv[2] ?? `target/debug/codync-host${process.platform === 'win32' ? '.exe' : ''}`)
const port = Number(process.argv[3] ?? 19223)
const home = mkdtempSync(join(tmpdir(), 'codync-flows-dev-'))
const base = `http://127.0.0.1:${port}`
const python = process.platform === 'win32' ? 'python' : 'python3'
const agent = resolve('tests/fixtures/team_agent.py')
const sleep = (ms) => new Promise((done) => setTimeout(done, ms))

async function free() {
  await new Promise((done, fail) => {
    const probe = net.createServer().once('error', () => fail(new Error(`port ${port} is in use: another host runs there; nothing was started`)))
    probe.listen(port, '127.0.0.1', () => probe.close(done))
  })
}

let host
async function start() {
  host = spawn(exe, ['serve', '--bind', '127.0.0.1', '--port', String(port)], {
    env: { ...process.env, CODYNC_HOME: home, CODYNC_CLOUD: 'off' },
    stdio: 'ignore',
    windowsHide: true,
  })
  for (let i = 0; i < 600; i++) {
    if (existsSync(join(home, 'token')) && (await call('routines', { botId: 'none' }, true).catch(() => null))) return
    await sleep(100)
  }
  throw new Error('the dev host did not start')
}
async function stop() {
  host.kill()
  await new Promise((done) => (host.exitCode === null ? host.once('exit', done) : done()))
}

async function call(method, body, soft = false) {
  const token = readFileSync(join(home, 'token'), 'utf8').trim()
  const response = await fetch(`${base}/api/${method}`, {
    method: 'POST',
    headers: { authorization: `Bearer ${token}`, 'content-type': 'application/json' },
    body: JSON.stringify(body),
  })
  const value = await response.json()
  if (soft) return value
  assert.ok(response.ok, `${method}: ${JSON.stringify(value)}`)
  return value
}

async function steps(bot, wanted, label) {
  for (let i = 0; i < 600; i++) {
    const canvas = await call('routineFlowCanvas', { botId: bot })
    const list = canvas.routines[0].steps
    const got = list.map((s) => `${s.botId.slice(0, 4)}:${s.status}`)
    if (wanted(list)) return list
    await sleep(100)
  }
  throw new Error(`steps never reached: ${label}`)
}

try {
  await free()
  await start()
  const bots = []
  for (const name of ['Ada', 'Ben', 'Cy']) {
    const cwd = join(home, name)
    mkdirSync(cwd)
    const command = `${python} -X utf8 -u "${agent}"`
    bots.push((await call('createBot', { name, backend: 'custom', command, cwd, permission: 'ask', notify: false })).bot.id)
  }
  const [a, b, c] = bots
  const routine = (await call('saveRoutine', { botId: a, name: 'Digest', instruction: 'Make the digest', triggers: [{ type: 'interval', seconds: 3600 }] })).routine.id
  const first = (await call('connectRoutineFlow', { routineId: routine, fromBotId: a, toBotId: b, instruction: 'Summarize' })).link.id
  await call('connectRoutineFlow', { routineId: routine, fromBotId: b, toBotId: c, instruction: 'Translate' })
  console.log('1. connected A -> B -> C')

  await call('testRoutineFlow', { botId: a, routineId: routine, input: 'Seven items today' })
  let list = await steps(a, (l) => l.length === 3 && l.every((s) => s.status === 'succeeded'), 'all succeeded')
  assert.equal(list[1].output, 'reply: Summarize')
  console.log('2. chain ok:', list.map((s) => s.status).join(', '))

  await call('updateRoutineFlowLink', { linkId: first, instruction: 'FAIL on purpose' })
  await call('testRoutineFlow', { botId: a, routineId: routine })
  list = await steps(a, (l) => l.length === 3 && l[1].status === 'failed' && l[2].status === 'skipped', 'B failed, C skipped')
  console.log('3. failure in B:', list.map((s) => s.status).join(', '), '| error:', list[1].error)

  await call('updateRoutineFlowLink', { linkId: first, instruction: 'BLOCK the work' })
  await call('testRoutineFlow', { botId: a, routineId: routine })
  await steps(a, (l) => l.some((s) => s.botId === b && s.status === 'running'), 'B running')
  await stop()
  await start()
  list = await steps(a, (l) => l.length === 3 && l[1].status === 'failed' && l[2].status === 'skipped', 'recovered after restart')
  assert.match(list[1].error, /restarted/)
  console.log('4. restart in mid-run recovered:', list.map((s) => s.status).join(', '), '| error:', list[1].error)
  console.log('OK')
} finally {
  if (host && host.exitCode === null) await stop()
  rmSync(home, { recursive: true, force: true })
}
