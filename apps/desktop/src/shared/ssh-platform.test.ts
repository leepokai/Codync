import assert from 'node:assert/strict'
import { once } from 'node:events'
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { createConnection, createServer } from 'node:net'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import test from 'node:test'
import { knownHostsFiles, ownsWindowsTunnel, presentedKeyArguments, tunnelArguments } from '../main/ssh-command.ts'
import { listenerCommand, run, sshListens, sshTools } from '../main/ssh-process.ts'
import { isAbsoluteKeyPath, validate } from '../main/ssh-profile.ts'
import type { SSHProfile } from './ssh.ts'

const profile: SSHProfile = { id: 'test', host: 'test-host', user: null, port: null, identityFile: null, remotePort: 19222, computerId: null, name: '' }

test('Windows uses native OpenSSH and normalizes quoted known_hosts paths', async () => {
  const tools = sshTools('win32', 'C:\\Windows')
  assert.equal(tools.ssh, 'C:\\Windows\\System32\\OpenSSH\\ssh.exe')
  assert.equal(tools.keygen, 'C:\\Windows\\System32\\OpenSSH\\ssh-keygen.exe')
  assert.equal(tools.keyscan, 'C:\\Windows\\System32\\OpenSSH\\ssh-keyscan.exe')
  assert.equal(sshTools('darwin').ssh, '/usr/bin/ssh')
  assert.equal(sshTools('linux').ssh, '/usr/bin/ssh')
  const home = 'C:\\Users\\Test User'
  assert.deepEqual(knownHostsFiles(home), ['C:/Users/Test User/.ssh/known_hosts', 'C:/Users/Test User/.codync/ssh_known_hosts'])
  const args = tunnelArguments(profile, 19333, home)
  const result = await run(sshTools().ssh, ['-G', '-F', process.platform === 'win32' ? 'NUL' : '/dev/null', ...args])
  assert.equal(result.status, 0, result.stderr)
  assert.match(result.stdout, /userknownhostsfile .*C:\/Users\/Test User\/\.ssh\/known_hosts.*C:\/Users\/Test User\/\.codync\/ssh_known_hosts/)
  assert.match(result.stdout, /stricthostkeychecking (true|yes)/)
  assert.match(result.stdout, /forwardagent no/)
  assert.match(result.stdout, /localforward (?:\[)?127\.0\.0\.1(?:\])?:19333 (?:\[)?127\.0\.0\.1(?:\])?:19222/)
})

test('Windows host key scan files the presented key in a scratch known_hosts and never signs in', async () => {
  const resolved = {
    hostname: '127.0.0.1', port: 2222, hostKeyAlias: null, proxyJump: null, proxyCommand: null,
    hostKeyAlgorithms: 'rsa-sha2-512,rsa-sha2-256', kexAlgorithms: 'curve25519-sha256',
  }
  const args = presentedKeyArguments(resolved, 'C:\\Users\\Test User\\Temp\\known_hosts')
  const result = await run(sshTools().ssh, ['-G', ...args])
  assert.equal(result.status, 0, result.stderr)
  assert.match(result.stdout, /^userknownhostsfile C:\/Users\/Test User\/Temp\/known_hosts$/m)
  assert.match(result.stdout, /^globalknownhostsfile C:\/Users\/Test User\/Temp\/known_hosts$/m)
  assert.match(result.stdout, /^stricthostkeychecking accept-new$/m)
  assert.match(result.stdout, /^preferredauthentications none$/m)
  assert.match(result.stdout, /^batchmode yes$/m)
  assert.match(result.stdout, /^port 2222$/m)
  assert.match(result.stdout, /^hostkeyalgorithms rsa-sha2-512,rsa-sha2-256$/m)
  assert.match(result.stdout, /^kexalgorithms curve25519-sha256$/m)
})

test('key selection accepts native absolute paths and rejects drive-relative paths', () => {
  for (const path of ['C:\\Users\\Test User\\key.pub', 'D:/SSH/key', '\\\\server\\share\\key']) assert.equal(isAbsoluteKeyPath(path, 'win32'), true, path)
  for (const path of ['key', 'C:key', '\\key', '/key', '~/key']) assert.equal(isAbsoluteKeyPath(path, 'win32'), false, path)
  assert.equal(isAbsoluteKeyPath('/Users/test/key', 'darwin'), true)
  const root = mkdtempSync(join(tmpdir(), 'codync-key-'))
  try {
    const key = join(root, 'key with spaces.pub')
    writeFileSync(key, 'public key fixture')
    assert.equal(validate({ ...profile, identityFile: key }), null)
    assert.match(validate({ ...profile, identityFile: root })!, /doesn't exist/)
    assert.match(validate({ ...profile, identityFile: join(root, 'missing') })!, /doesn't exist/)
  } finally { rmSync(root, { recursive: true, force: true }) }
})

test('Windows orphan cleanup recognizes only this variant and its native ssh executable', () => {
  const binary = 'C:\\Windows\\System32\\OpenSSH\\ssh.exe'
  const dir = 'C:\\Users\\Test User\\.codync'
  const options = String.raw`"UserKnownHostsFile=\"C:/Users/Test User/.ssh/known_hosts\" \"C:/Users/Test User/.codync/ssh_known_hosts\""`
  const owned = { ProcessId: 123, ExecutablePath: binary, CommandLine: `"${binary}" -N -T -o ${options} -L 127.0.0.1:19333:127.0.0.1:19222 -- test-host` }
  assert.equal(ownsWindowsTunnel(owned, binary, dir), true)
  assert.equal(ownsWindowsTunnel(owned, binary, `${dir}-dev`), false)
  assert.equal(ownsWindowsTunnel(owned, binary, dir.replace('Test User', 'Other User')), false)
  assert.equal(ownsWindowsTunnel({ ...owned, ExecutablePath: 'C:\\Other\\ssh.exe' }, binary, dir), false)
  assert.equal(ownsWindowsTunnel({ ...owned, CommandLine: owned.CommandLine.replace(' -N ', ' ') }, binary, dir), false)
  assert.equal(ownsWindowsTunnel({ ...owned, CommandLine: owned.CommandLine.replace('ssh_known_hosts', 'ssh_known_hosts_other') }, binary, dir), false)
  assert.equal(ownsWindowsTunnel({ ...owned, CommandLine: null }, binary, dir), false)
})

test('listener ownership accepts the listening process and rejects another process', async () => {
  const server = createServer()
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve))
  try {
    const address = server.address()
    assert.ok(address && typeof address === 'object')
    const ownListener = await sshListens(process.pid, address.port)
    const otherListener = await sshListens(process.pid === 1 ? 2 : 1, address.port)
    assert.equal(ownListener, true)
    assert.equal(otherListener, false)
  } finally { await new Promise<void>((resolve) => server.close(() => resolve())) }
  assert.throws(() => listenerCommand(123, -1))
  assert.throws(() => listenerCommand(NaN, 19222))
})

test('listener ownership rejects an outgoing socket and a closed port', async () => {
  const server = createServer((socket) => socket.resume())
  await new Promise<void>((resolve) => server.listen(0, '127.0.0.1', resolve))
  const address = server.address()
  assert.ok(address && typeof address === 'object')
  const client = createConnection({ host: '127.0.0.1', port: address.port })
  try {
    await once(client, 'connect')
    assert.ok(client.localPort)
    const outgoing = await sshListens(process.pid, client.localPort)
    assert.equal(outgoing, false)
  } finally {
    client.destroy()
    await new Promise<void>((resolve) => server.close(() => resolve()))
  }
  const closed = await sshListens(process.pid, address.port)
  assert.equal(closed, false)
})
