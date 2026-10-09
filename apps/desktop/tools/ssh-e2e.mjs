// Read-only integration checks against a real SSH host. No bots or settings are changed.
// node --experimental-strip-types tools/ssh-e2e.mjs <ssh-alias>
import assert from 'node:assert/strict'
import { spawn, spawnSync } from 'node:child_process'
import { once } from 'node:events'
import { createServer } from 'node:net'
import { homedir } from 'node:os'
import { setTimeout as delay } from 'node:timers/promises'
import { infoArguments, tunnelArguments } from '../src/main/ssh-command.ts'

const host = process.argv[2]
if (!host || host.startsWith('-') || !/^[A-Za-z0-9._-]+$/.test(host)) {
  throw new Error('Pass the SSH alias of a trusted, already configured test computer.')
}
const profile = {
  id: 'e2e', host, user: null, port: null, identityFile: null,
  remotePort: 19222, computerId: null, name: 'SSH integration test',
}
const children = new Set()
process.on('exit', () => { for (const child of children) child.kill() })

async function listeningServer() {
  const server = createServer()
  server.listen(0, '127.0.0.1')
  await once(server, 'listening')
  return { server, port: server.address().port }
}
const close = (server) => new Promise((resolve, reject) => server.close((error) => error ? reject(error) : resolve()))
const listeners = (port, pid) => spawnSync('/usr/sbin/lsof', [
  '-a', '-n', '-P', ...(pid ? ['-p', String(pid)] : []),
  `-iTCP@127.0.0.1:${port}`, '-sTCP:LISTEN', '-t',
], { encoding: 'utf8' }).stdout.trim()

async function healthThrough(base, child) {
  for (let attempt = 0; attempt < 50; attempt++) {
    assert.equal(child.exitCode, null, 'SSH must stay alive and own its tunnel')
    try {
      const response = await fetch(`${base}/health`, { signal: AbortSignal.timeout(500) })
      if (response.ok) return await response.json()
    } catch {
      // The listener may not yet exist while SSH authenticates.
    }
    await delay(100)
  }
  throw new Error('Tunnel did not become healthy within the startup deadline')
}

async function api(base, token, method, body = {}) {
  return fetch(`${base}/api/${method}`, {
    method: 'POST', body: JSON.stringify(body), signal: AbortSignal.timeout(5000),
    headers: { 'Content-Type': 'application/json', ...(token ? { Authorization: `Bearer ${token}` } : {}) },
  })
}

async function verifyEvents(base, token) {
  const controller = new AbortController()
  const timer = setTimeout(() => controller.abort(), 5000)
  try {
    const response = await fetch(`${base}/events?since=0&client=codync-ssh-e2e`, {
      headers: { Authorization: `Bearer ${token}` }, signal: controller.signal,
    })
    assert.equal(response.status, 200)
    const reader = response.body.getReader()
    const decoder = new TextDecoder()
    let pending = ''
    for (;;) {
      const { value, done } = await reader.read()
      assert.equal(done, false, 'SSE must deliver hello before closing')
      pending += decoder.decode(value, { stream: true })
      let newline
      while ((newline = pending.indexOf('\n')) !== -1) {
        const line = pending.slice(0, newline)
        pending = pending.slice(newline + 1)
        if (line.startsWith('data:') && JSON.parse(line.slice(5)).type === 'hello') {
          await reader.cancel()
          return
        }
      }
    }
  } finally {
    clearTimeout(timer)
    controller.abort()
  }
}

const result = spawnSync('/usr/bin/ssh', infoArguments(profile, homedir()), { encoding: 'utf8', timeout: 20_000 })
assert.equal(result.status, 0, result.stderr)
const info = JSON.parse(result.stdout)
assert.equal(info.running, true)
console.log(`PASS remote discovery: host version ${info.version}`)

for (let round = 1; round <= 2; round++) {
  const { server, port } = await listeningServer()
  await close(server)
  const child = spawn('/usr/bin/ssh', tunnelArguments(profile, port, homedir()), { stdio: ['ignore', 'ignore', 'pipe'] })
  children.add(child)
  let error = ''
  child.stderr.on('data', (data) => { error = (error + data).slice(-4096) })
  const exited = once(child, 'close')
  const base = `http://127.0.0.1:${port}`
  try {
    const health = await healthThrough(base, child)
    assert.equal(health.computerId, info.computerId, error)
    assert.equal(listeners(port, child.pid), String(child.pid))
    const denied = await api(base, null, 'hello')
    assert.equal(denied.status, 401)
    await denied.body?.cancel()
    const helloResponse = await api(base, info.token, 'hello')
    assert.equal(helloResponse.status, 200)
    const hello = await helloResponse.json()
    assert.equal(hello.computerId, info.computerId)
    const syncResponse = await api(base, info.token, 'sync', { since: 0 })
    assert.equal(syncResponse.status, 200)
    const sync = await syncResponse.json()
    assert.ok(Array.isArray(sync.bots))
    await verifyEvents(base, info.token)
    console.log(`PASS round ${round}: owned listener, host identity, authentication, ${sync.bots.length} bots, SSE hello`)
  } finally {
    child.kill()
    await exited
    children.delete(child)
  }
  assert.equal(listeners(port), '')
  console.log(`PASS round ${round}: disconnect releases the listener`)
}

const { server, port } = await listeningServer()
try {
  const collision = spawnSync('/usr/bin/ssh', tunnelArguments(profile, port, homedir()), { encoding: 'utf8', timeout: 15_000 })
  assert.notEqual(collision.status, 0)
  assert.match(collision.stderr, /Address already in use|cannot listen to port/)
  assert.equal(server.listening, true)
  console.log('PASS occupied local port fails without replacing the existing listener')
} finally {
  await close(server)
}
