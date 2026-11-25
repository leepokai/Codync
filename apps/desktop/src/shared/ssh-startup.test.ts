import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { HostState } from './ipc.ts'
import { isSSHConnecting, showsChat, sshSummary, sshStatusDetail } from './ssh-startup.ts'
import { sshMessage, type SSHProfile, type SSHState, type SSHStatus } from './ssh.ts'

const profile: SSHProfile = { id: 'remote', host: 'remote', name: 'Remote', port: null, user: null, identityFile: null, remotePort: 19222, computerId: null }
const en = (text: string) => text
const state = (status?: SSHStatus): SSHState => ({ profiles: [profile], status: status ? { remote: status } : {}, attachments: [], needsLinger: [] })

test('saved profiles keep connection status and management visible in every state', () => {
  const statuses: SSHStatus[] = [
    { kind: 'idle' }, { kind: 'failed', message: { phrase: 'Permission denied' } }, { kind: 'notInstalled' },
    { kind: 'confirmHostKey', name: 'remote', fingerprints: [] }, { kind: 'connected', localPort: 19333 },
  ]
  for (const host of ['notInstalled', 'failed', 'missingBinary'] satisfies HostState['kind'][]) {
    for (const status of statuses) assert.equal(showsChat(host, 0, state(status)), true, `${host}/${status.kind}`)
    assert.equal(showsChat(host, 0, state()), true, `${host}/missing SSH status`)
    assert.equal(showsChat(host, 0, { profiles: [], status: {}, attachments: [], needsLinger: [] }), false)
  }
})

test('connecting or retrying SSH profiles open the workspace without a local host', () => {
  for (const status of [{ kind: 'connecting', step: 'Signing in…' }, { kind: 'retrying', message: { phrase: 'Host unreachable' } }] satisfies SSHStatus[]) {
    assert.equal(isSSHConnecting(status), true)
    assert.equal(showsChat('notInstalled', 0, state(status)), true)
    assert.equal(showsChat('failed', 0, state(status)), true)
  }
})

test('an available computer or running local host keeps the workspace open', () => {
  const failed = state({ kind: 'failed', message: { phrase: 'Permission denied' } })
  assert.equal(showsChat('running', 0, failed), true)
  assert.equal(showsChat('notInstalled', 1, failed), true)
})

test('pending SSH rows distinguish idle, connected, retrying and terminal failures', () => {
  assert.equal(sshStatusDetail({ kind: 'idle' }, true, en), 'Not connected')
  assert.equal(sshStatusDetail({ kind: 'connected', localPort: 19333 }, true, en), 'Connected')
  assert.equal(sshStatusDetail({ kind: 'connected', localPort: 19333 }, false, en), 'Connected · opening…')
  assert.equal(sshStatusDetail({ kind: 'connecting', step: 'Opening the tunnel…' }, true, en), 'Opening the tunnel…')
  assert.equal(sshStatusDetail({ kind: 'retrying', message: { phrase: 'Host unreachable' } }, true, en), 'Host unreachable Retrying…')
  assert.equal(sshStatusDetail({ kind: 'notInstalled' }, true, en), 'Host not installed')
  assert.equal(sshStatusDetail({ kind: 'failed', message: { phrase: 'Permission denied' } }, true, en), 'Permission denied')
  assert.equal(sshStatusDetail({ kind: 'confirmHostKey', name: 'remote', fingerprints: [] }, true, en), 'Confirm host key…')
})

test('summary separates active connection attempts from profiles needing attention', () => {
  assert.equal(sshSummary([{ kind: 'failed', message: { phrase: 'Permission denied' } }, { kind: 'confirmHostKey', name: 'remote', fingerprints: [] }, { kind: 'idle' }], en), '2 need attention · 1 not connected')
  assert.equal(sshSummary([{ kind: 'connecting', step: 'Signing in…' }, { kind: 'retrying', message: { phrase: 'Host unreachable' } }, { kind: 'connected', localPort: 19333 }], en), '2 connecting… · 1 opening…')
  assert.equal(sshSummary([{ kind: 'notInstalled' }], en), '1 needs attention')
  assert.equal(sshSummary([], en), '')
})

test('an install in progress counts as connecting and shows its step', () => {
  const installing: SSHStatus = { kind: 'installing', step: 'Starting codync-host…' }
  assert.equal(isSSHConnecting(installing), true)
  assert.equal(sshStatusDetail(installing, true, en), 'Starting codync-host…')
  assert.equal(sshSummary([installing], en), '1 connecting…')
  assert.equal(showsChat('notInstalled', 0, state(installing)), true)
  // A failed install leaves the computer not installed, so Install is offered again.
  assert.equal(sshSummary([{ kind: 'notInstalled', failure: { output: 'error: download failed' } }], en), '1 needs attention')
})

test('every phrase of the summary goes through the catalogs (#517)', () => {
  const tr = (text: string) => `‹${text}›`
  const summary = sshSummary([
    { kind: 'connecting', step: 'Signing in…' }, { kind: 'failed', message: { phrase: 'Permission denied' } },
    { kind: 'connected', localPort: 19333 }, { kind: 'idle' },
  ], tr)
  const parts = summary.split(' · ')
  assert.equal(parts.length, 4)
  for (const part of parts) assert.match(part, /^‹.*›$/, `untranslated: ${part}`)
})

test('every fixed phrase of status detail goes through the catalogs and splits Retrying… (#521)', () => {
  const tr = (text: string) => `‹${text}›`
  assert.equal(sshStatusDetail({ kind: 'idle' }, false, tr), '‹Not connected›')
  assert.equal(sshStatusDetail({ kind: 'connecting', step: 'Reading the host key…' }, false, tr), '‹Reading the host key…›')
  assert.equal(sshStatusDetail({ kind: 'installing', step: 'Starting codync-host…' }, false, tr), '‹Starting codync-host…›')
  assert.equal(sshStatusDetail({ kind: 'confirmHostKey', name: 'remote', fingerprints: [] }, false, tr), '‹Confirm host key…›')
  assert.equal(sshStatusDetail({ kind: 'connected', localPort: 19333 }, true, tr), '‹Connected›')
  assert.equal(sshStatusDetail({ kind: 'connected', localPort: 19333 }, false, tr), '‹Connected · opening…›')
  assert.equal(sshStatusDetail({ kind: 'notInstalled' }, false, tr), '‹Host not installed›')
  assert.equal(sshStatusDetail({ kind: 'retrying', message: { phrase: 'Host unreachable' } }, false, tr), '‹Host unreachable› ‹Retrying…›')
  assert.equal(sshStatusDetail({ kind: 'failed', message: { phrase: 'Permission denied' } }, false, tr), '‹Permission denied›')
})

test('retrying and failed details translate the phrase, fill its values and keep what ssh printed as written', () => {
  const zh: Record<string, string> = {
    "SSH couldn't connect to {name}.": 'SSH 無法連線到 {name}。',
    'Retrying…': '正在重試…',
    'The host key of {name} changed.': '{name} 的主機金鑰已變更。',
  }
  const tr = (text: string) => zh[text] ?? text
  const retrying: SSHStatus = { kind: 'retrying', message: sshMessage("SSH couldn't connect to {name}.", { name: 'mini' }, 'ssh: connect to host mini port 22: Connection refused') }
  assert.equal(sshStatusDetail(retrying, false, tr), 'SSH 無法連線到 mini。 ssh: connect to host mini port 22: Connection refused 正在重試…')
  const failed: SSHStatus = { kind: 'failed', message: sshMessage('The host key of {name} changed.', { name: 'mini' }) }
  assert.equal(sshStatusDetail(failed, false, tr), 'mini 的主機金鑰已變更。')
})
