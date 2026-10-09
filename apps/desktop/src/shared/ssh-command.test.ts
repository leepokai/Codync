import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { test } from 'node:test'
import { infoArguments, remoteInfoCommand, resolveArguments, tunnelArguments } from '../main/ssh-command.ts'
import type { SSHProfile } from './ssh.ts'

// Remap absolute installation directories into an isolated filesystem fixture. Run the
// command inside sh -c so the machine's login scripts cannot supply an installed host.
const locations = [
  '/custom/bin',
  '/opt/homebrew/bin',
  '/usr/local/bin',
  '/home/linuxbrew/.linuxbrew/bin',
  '/user home/.local/bin',
  '/Applications/Codync.app/Contents/Resources',
  '/Applications/Codync.app/Contents/MacOS',
]

function probe(installed: string[]) {
  const root = mkdtempSync(join(tmpdir(), 'codync-ssh-'))
  try {
    for (const location of installed) {
      const directory = join(root, location)
      mkdirSync(directory, { recursive: true })
      const binary = join(directory, 'codync-host')
      writeFileSync(binary, '#!/bin/sh\nprintf "%s\\n" "$0" "$@"\n')
      chmodSync(binary, 0o755)
    }
    const command = remoteInfoCommand(19222)
      .replace(/^sh -lc '/, '').replace(/'$/, '')
      .replace(/:(\/[^:"]+)/g, (_, path: string) => `:${root}${path}`)
    const result = spawnSync('/bin/sh', ['-c', command], {
      encoding: 'utf8',
      env: { HOME: join(root, 'user home'), PATH: join(root, 'custom/bin') },
    })
    return { status: result.status, lines: result.stdout.trim().split('\n'), root }
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
}

for (const location of locations) {
  test(`SSH finds a host installed in ${location}`, { skip: process.platform === 'win32' }, () => {
    const result = probe([location])
    assert.equal(result.status, 0)
    assert.deepEqual(result.lines, [join(result.root, location, 'codync-host'), 'info', '--json', '--port', '19222'])
  })
}

test('SSH preserves the host selected by the remote PATH', { skip: process.platform === 'win32' }, () => {
  const result = probe(locations)
  assert.equal(result.status, 0)
  assert.equal(result.lines[0], join(result.root, 'custom/bin/codync-host'))
})

test('SSH reports command not found when no installation is reachable', { skip: process.platform === 'win32' }, () => {
  assert.equal(probe([]).status, 127)
})

for (const installed of ['mac', 'linux', 'production-only'] as const) {
  test(`development SSH uses only its own host with ${installed} installed`, { skip: process.platform === 'win32' }, () => {
    const root = mkdtempSync(join(tmpdir(), 'codync-dev-ssh-'))
    try {
      const home = join(root, 'user home')
      const mac = join(root, 'Applications/Codync Dev.app/Contents/Resources/codync-host')
      const linux = join(home, '.local/bin/codync-dev-host')
      const prod = join(home, '.local/bin/codync-host')
      for (const bin of [prod, ...(installed === 'mac' ? [mac] : installed === 'linux' ? [linux] : [])]) {
        mkdirSync(join(bin, '..'), { recursive: true })
        writeFileSync(bin, '#!/bin/sh\nprintf "%s\\n" "$0" "$CODYNC_HOME" "$@"\n')
        chmodSync(bin, 0o755)
      }
      const command = remoteInfoCommand(19223, '.codync-dev')
        .replace(/^sh -lc '/, '').replace(/'$/, '')
        .replaceAll('/Applications/', `${root}/Applications/`)
      const result = spawnSync('/bin/sh', ['-c', command], {
        encoding: 'utf8', env: { HOME: home, PATH: join(home, '.local/bin') },
      })
      assert.equal(result.status, installed === 'production-only' ? 127 : 0)
      if (installed !== 'production-only') {
        assert.deepEqual(result.stdout.trim().split('\n'), [installed === 'mac' ? mac : linux, join(home, '.codync-dev'), 'info', '--json', '--port', '19223'])
      }
    } finally {
      rmSync(root, { recursive: true, force: true })
    }
  })
}

const profile: SSHProfile = {
  id: 'test', host: 'test-mini', user: null, port: null, identityFile: null,
  remotePort: 19222, computerId: null, name: 'Test mini',
}

function withConfig(master: string, run: (config: string, home: string) => void) {
  const root = mkdtempSync(join(tmpdir(), 'codync-ssh-config-'))
  try {
    const config = join(root, 'config')
    writeFileSync(config, `Host test-mini
  HostName 192.0.2.1
  User test-user
  Port 2222
Host *
  ControlMaster ${master}
  ControlPath ${root}/master-%C
  ControlPersist 4h
  ForwardAgent yes
  ForwardX11 yes
  StrictHostKeyChecking no
`)
    run(config, join(root, 'user home'))
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
}

function resolved(config: string, args: string[]) {
  const result = spawnSync('/usr/bin/ssh', ['-G', '-F', config, ...args], { encoding: 'utf8' })
  assert.equal(result.status, 0, result.stderr)
  return new Map(result.stdout.trim().split('\n').map((line) => {
    const at = line.indexOf(' ')
    return [line.slice(0, at), line.slice(at + 1)]
  }))
}

for (const master of ['no', 'auto', 'yes', 'autoask']) {
  test(`managed tunnels own their connection with ControlMaster ${master}`, { skip: process.platform === 'win32' }, () => {
    withConfig(master, (config, home) => {
      const values = resolved(config, tunnelArguments(profile, 19333, home))
      // OpenSSH omits ControlPath entirely for -S none; there is no master to reuse.
      assert.equal(values.get('controlpath'), undefined)
      assert.equal(values.get('hostname'), '192.0.2.1')
      assert.equal(values.get('user'), 'test-user')
      assert.equal(values.get('port'), '2222')
      assert.equal(values.get('forwardagent'), 'no')
      assert.equal(values.get('forwardx11'), 'no')
      assert.equal(values.get('stricthostkeychecking'), 'true')
      assert.equal(values.get('exitonforwardfailure'), 'yes')
      assert.equal(values.get('batchmode'), 'yes')
      assert.equal(values.get('localforward')?.replace(/[\[\]]/g, ''), '127.0.0.1:19333 127.0.0.1:19222')
      assert.ok(values.get('userknownhostsfile')?.includes(`${home}/.ssh/known_hosts`))
      assert.ok(values.get('userknownhostsfile')?.includes(`${home}/.codync/ssh_known_hosts`))
    })
  })
}

for (const kind of ['info', 'tunnel', 'resolve'] as const) {
  test(`${kind} honors explicit user and port overrides`, { skip: process.platform === 'win32' }, () => {
    withConfig('auto', (config, home) => {
      mkdirSync(home)
      const key = join(home, 'key with spaces')
      writeFileSync(key, '')
      const overridden = { ...profile, user: 'override', port: 2201, remotePort: 19444, identityFile: key }
      const args = kind === 'info' ? infoArguments(overridden, home)
        : kind === 'tunnel' ? tunnelArguments(overridden, 19555, home) : resolveArguments(overridden)
      const values = resolved(config, args)
      assert.equal(values.get('user'), 'override')
      assert.equal(values.get('port'), '2201')
      assert.equal(values.get('hostname'), '192.0.2.1')
      if (kind !== 'resolve') assert.equal(values.get('identityfile'), key)
      if (kind === 'tunnel') assert.equal(values.get('localforward')?.replace(/[\[\]]/g, ''), '127.0.0.1:19555 127.0.0.1:19444')
      if (kind === 'info') assert.match(args.at(-1)!, /info --json --port 19444/)
    })
  })
}

for (const [preferred, fallback] of [
  ['/opt/homebrew/bin', '/Applications/Codync.app/Contents/Resources'],
  ['/Applications/Codync.app/Contents/Resources', '/Applications/Codync.app/Contents/MacOS'],
]) {
  test(`SSH prefers ${preferred} over ${fallback}`, { skip: process.platform === 'win32' }, () => {
    const result = probe([preferred, fallback])
    assert.equal(result.status, 0)
    assert.equal(result.lines[0], join(result.root, preferred, 'codync-host'))
  })
}
