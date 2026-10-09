import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { HostState } from './ipc.ts'
import { isSSHConnecting, showsChat, sshSummary, sshStatusDetail } from './ssh-startup.ts'
import type { SSHProfile, SSHState, SSHStatus } from './ssh.ts'

const profile: SSHProfile = { id: 'remote', host: 'remote', name: 'Remote', port: null, user: null, identityFile: null, remotePort: 19222, computerId: null }
const state = (status?: SSHStatus): SSHState => ({ profiles: [profile], status: status ? { remote: status } : {}, attachments: [] })

test('saved profiles keep connection status and management visible in every state', () => {
  const statuses: SSHStatus[] = [
    { kind: 'idle' }, { kind: 'failed', message: 'Permission denied' }, { kind: 'notInstalled' },
    { kind: 'confirmHostKey', name: 'remote', fingerprints: [] }, { kind: 'connected', localPort: 19333 },
  ]
  for (const host of ['notInstalled', 'failed', 'missingBinary'] satisfies HostState['kind'][]) {
    for (const status of statuses) assert.equal(showsChat(host, 0, state(status)), true, `${host}/${status.kind}`)
    assert.equal(showsChat(host, 0, state()), true, `${host}/missing SSH status`)
    assert.equal(showsChat(host, 0, { profiles: [], status: {}, attachments: [] }), false)
  }
})

test('connecting or retrying SSH profiles open the workspace without a local host', () => {
  for (const status of [{ kind: 'connecting', step: 'Signing in…' }, { kind: 'retrying', message: 'Host unreachable' }] satisfies SSHStatus[]) {
    assert.equal(isSSHConnecting(status), true)
    assert.equal(showsChat('notInstalled', 0, state(status)), true)
    assert.equal(showsChat('failed', 0, state(status)), true)
  }
})

test('an available computer or running local host keeps the workspace open', () => {
  const failed = state({ kind: 'failed', message: 'Permission denied' })
  assert.equal(showsChat('running', 0, failed), true)
  assert.equal(showsChat('notInstalled', 1, failed), true)
})

test('pending SSH rows distinguish idle, connected, retrying and terminal failures', () => {
  assert.equal(sshStatusDetail({ kind: 'idle' }), 'Not connected')
  assert.equal(sshStatusDetail({ kind: 'connected', localPort: 19333 }), 'Connected')
  assert.equal(sshStatusDetail({ kind: 'connected', localPort: 19333 }, false), 'Connected · opening…')
  assert.equal(sshStatusDetail({ kind: 'connecting', step: 'Opening the tunnel…' }), 'Opening the tunnel…')
  assert.equal(sshStatusDetail({ kind: 'retrying', message: 'Host unreachable' }), 'Host unreachable Retrying…')
  assert.equal(sshStatusDetail({ kind: 'notInstalled' }), 'Host not installed')
  assert.equal(sshStatusDetail({ kind: 'failed', message: 'Permission denied' }), 'Permission denied')
  assert.equal(sshStatusDetail({ kind: 'confirmHostKey', name: 'remote', fingerprints: [] }), 'Confirm host key…')
})

test('summary separates active connection attempts from profiles needing attention', () => {
  assert.equal(sshSummary([{ kind: 'failed', message: 'Permission denied' }, { kind: 'confirmHostKey', name: 'remote', fingerprints: [] }, { kind: 'idle' }]), '2 need attention · 1 not connected')
  assert.equal(sshSummary([{ kind: 'connecting', step: 'Signing in…' }, { kind: 'retrying', message: 'Host unreachable' }, { kind: 'connected', localPort: 19333 }]), '2 connecting… · 1 opening…')
  assert.equal(sshSummary([{ kind: 'notInstalled' }]), '1 needs attention')
  assert.equal(sshSummary([]), '')
})
