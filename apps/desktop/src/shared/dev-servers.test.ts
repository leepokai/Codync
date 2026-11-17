import assert from 'node:assert/strict'
import { test } from 'node:test'
import { devServerAddress, devServerAddressTitle, devServerClick, devServerMenuRows, devServerRemote, devServerSummary, devServerValue, type DevServer } from './dev-servers.ts'
import { normalizeBot } from './models.ts'

const server = (over: Partial<DevServer>): DevServer => ({
  id: 'a', name: 'web', command: 'npm run dev', folder: '/site', status: 'running', ports: [], startedAt: 1, exitCode: null, managed: true, ...over,
})
const port = (n: number, remote: string[] = []) => ({ port: n, local: `http://localhost:${n}`, remote })
const tailscale = 'http://100.64.0.1:5173'
const lan = 'http://192.168.1.20:5173'

test('the title shows nothing without a live server', () => {
  assert.deepEqual(devServerSummary([]), { kind: 'none' })
  assert.deepEqual(devServerSummary([server({ status: 'stopped' })]), { kind: 'none' })
})

test('the title shows the port, and how many more run', () => {
  assert.deepEqual(devServerSummary([server({ ports: [port(5173)] })]), { kind: 'running', text: ':5173' })
  const servers = [server({ name: 'watch' }), server({ ports: [port(5173)] }), server({ ports: [port(8787)] })]
  assert.deepEqual(devServerSummary(servers), { kind: 'running', text: ':5173 +2' })
})

test('the title names a server without a port', () => {
  assert.deepEqual(devServerSummary([server({ name: 'watch' })]), { kind: 'running', text: 'watch' })
})

test('a running server outranks one starting, which outranks one failed', () => {
  assert.deepEqual(devServerSummary([server({ status: 'failed' }), server({ ports: [port(1)] })]), { kind: 'running', text: ':1' })
  assert.deepEqual(devServerSummary([server({ status: 'failed' }), server({ status: 'starting' })]), { kind: 'starting' })
  assert.deepEqual(devServerSummary([server({ status: 'failed' })]), { kind: 'failed' })
})

test('a server opens at its local address on its own computer', () => {
  assert.equal(devServerAddress(server({ ports: [port(5173)] }), true), 'http://localhost:5173')
})

test('elsewhere, an SSH store included, a server opens only at what other devices can reach', () => {
  assert.equal(devServerAddress(server({ ports: [port(5173)] }), false), null)
  assert.equal(devServerAddress(server({ ports: [port(5173, [lan])] }), false), lan)
  assert.equal(devServerAddress(server({ ports: [port(1), port(5173, [lan])] }), false), lan, 'the first port listens only on the computer')
})

test('elsewhere, a click opens the first address, and several are each a row of the menu', () => {
  const two = server({ ports: [port(5173, [tailscale, lan])] })
  assert.deepEqual(devServerClick(two, false), { kind: 'open', address: tailscale })
  assert.deepEqual(devServerRemote(two), [tailscale, lan])
  assert.deepEqual(devServerMenuRows(two, false), [{ open: tailscale }, { open: lan }, 'copyAddress', 'logs', 'restart', 'stop'])
  assert.deepEqual(devServerMenuRows(server({ ports: [port(1), port(5173, [tailscale, lan])] }), false).slice(0, 2), [{ open: tailscale }, { open: lan }])
  assert.equal(devServerAddress(two, false), tailscale, 'Copy address copies the first')
  assert.deepEqual(devServerMenuRows(server({ ports: [port(5173, [lan])] }), false), ['copyAddress', 'logs', 'restart', 'stop'], 'one has nothing to choose')
  assert.deepEqual(devServerMenuRows(server({ status: 'stopped', ports: [port(5173, [tailscale, lan])] }), false), ['logs', 'start', 'remove'], 'a stopped server has no address')
})

test('on its own computer a server opens at local, with no rows for its addresses', () => {
  const two = server({ ports: [port(5173, [tailscale, lan])] })
  assert.equal(devServerAddress(two, true), 'http://localhost:5173')
  assert.deepEqual(devServerMenuRows(two, true), ['copyAddress', 'logs', 'restart', 'stop'])
})

test('a row names an address without its scheme', () => {
  assert.equal(devServerAddressTitle(lan), '192.168.1.20:5173')
})

test('only a running server with a port opens', () => {
  assert.equal(devServerAddress(server({ status: 'stopped', ports: [port(5173)] }), true), null)
  assert.equal(devServerAddress(server({}), true), null)
})

test('a row says its ports or its state', () => {
  assert.equal(devServerValue(server({ ports: [port(5173), port(24678)] })), ':5173 :24678')
  assert.equal(devServerValue(server({})), 'Running')
  assert.equal(devServerValue(server({ status: 'starting' })), 'Starting')
  assert.equal(devServerValue(server({ status: 'stopped' })), 'Stopped')
  assert.equal(devServerValue(server({ status: 'failed' })), 'Failed to start')
})

test('a click opens what can be reached from here, and reads the log of what has no port', () => {
  assert.deepEqual(devServerClick(server({ ports: [port(5173)] }), true), { kind: 'open', address: 'http://localhost:5173' })
  assert.deepEqual(devServerClick(server({ ports: [port(5173)] }), false), { kind: 'menu' }, 'its menu says why not')
  for (const status of ['running', 'starting', 'stopped', 'failed'] as const) {
    assert.deepEqual(devServerClick(server({ status }), true), { kind: 'logs' }, status)
  }
  assert.deepEqual(devServerClick(server({ managed: false }), true), { kind: 'menu' }, 'one from the shell has no log')
})

test('the menu offers only what applies', () => {
  assert.deepEqual(devServerMenuRows(server({ ports: [port(5173)] }), true), ['copyAddress', 'logs', 'restart', 'stop'])
  assert.deepEqual(devServerMenuRows(server({}), true), ['logs', 'restart', 'stop'])
  assert.deepEqual(devServerMenuRows(server({ status: 'starting' }), true), ['logs', 'stop'])
  assert.deepEqual(devServerMenuRows(server({ status: 'stopped' }), true), ['logs', 'start', 'remove'])
  assert.deepEqual(devServerMenuRows(server({ status: 'failed' }), true), ['logs', 'start', 'remove'])
  assert.deepEqual(devServerMenuRows(server({ ports: [port(5173)], managed: false }), true), ['copyAddress', 'stop'])
})

test('a status from a newer host reads as stopped', () => {
  const raw = { id: 'a', status: 'sleeping' } as unknown as DevServer
  const bot = normalizeBot({ id: 'b', devServers: [raw, server({ status: 'failed' })] })
  assert.deepEqual(bot.devServers.map((s) => s.status), ['stopped', 'failed'])
  assert.deepEqual(normalizeBot({ id: 'b' }).devServers, [], 'an older host sends none')
})
