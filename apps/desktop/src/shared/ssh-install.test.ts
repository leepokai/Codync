import assert from 'node:assert/strict'
import { spawnSync } from 'node:child_process'
import { chmodSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
import { test } from 'node:test'
import { LINGER_NEEDED, installArguments, installOutput, installSteps, needsLinger, remoteInstallCommand, type InstallStep } from '../main/ssh-command.ts'
import type { SSHProfile } from './ssh.ts'

const skip = process.platform === 'win32'

function tool(dir: string, name: string, body: string) {
  mkdirSync(dir, { recursive: true })
  writeFileSync(join(dir, name), `#!/bin/sh\n${body}\n`)
  chmodSync(join(dir, name), 0o755)
}

/**
 * Runs an install step's remote command under `sh -c` in a fixture home, with fake `curl`, `uname`, `loginctl`
 * and `codync-host` first on PATH: `env` picks how they answer.
 */
function remote(step: InstallStep, env: Record<string, string>) {
  const root = mkdtempSync(join(tmpdir(), 'codync-install-'))
  try {
    const home = join(root, 'home')
    const bin = join(root, 'bin')
    // The fake install.sh records where it was told to install and its options.
    tool(bin, 'curl', '[ "$CURL_FAILS" = 1 ] && exit 22\nprintf \'%s\\n\' \'printf "%s\\n" "$CODYNC_BIN_DIR" "$@" > "$HOME/script-args"\'')
    tool(bin, 'uname', 'echo "$OS"')
    tool(bin, 'loginctl', 'case $1 in show-user) echo "$LINGER" ;; --no-ask-password) [ "$2" = enable-linger ] && exit "$ENABLE_EXIT" ;; esac\nexit 1')
    tool(join(home, '.local/bin'), 'codync-host', 'echo "$@" > "$HOME/host-args"\nexit "$HOST_EXIT"')
    const command = remoteInstallCommand(step, 19333).replace(/^sh -lc '/, '').replace(/'$/, '')
    const result = spawnSync('/bin/sh', ['-c', command], {
      encoding: 'utf8',
      env: { HOME: home, PATH: `${bin}:/usr/bin:/bin`, OS: 'Linux', LINGER: 'no', ENABLE_EXIT: '1', HOST_EXIT: '0', ...env },
    })
    const read = (name: string) => { try { return readFileSync(join(home, name), 'utf8') } catch { return null } }
    return { status: result.status, stdout: result.stdout, scriptArgs: read('script-args'), hostArgs: read('host-args'), home }
  } finally {
    rmSync(root, { recursive: true, force: true })
  }
}

test('the download step runs install.sh for the host only, into ~/.local/bin', { skip }, () => {
  const r = remote('download', {})
  assert.equal(r.status, 0)
  assert.equal(r.scriptArgs, `${join(r.home, '.local/bin')}\n--host-only\n`)
})

test('a failed download fails the step instead of running an empty script', { skip }, () => {
  const r = remote('download', { CURL_FAILS: '1' })
  assert.notEqual(r.status, 0)
  assert.equal(r.scriptArgs, null)
})

test('the service step installs the service on the profile port', { skip }, () => {
  const r = remote('service', { LINGER: 'yes' })
  assert.equal(r.status, 0)
  assert.equal(r.hostArgs, 'install --port 19333\n')
  assert.equal(needsLinger(r.stdout), false)
})

test('Linux without linger asks for the sudo command only when enabling it is refused', { skip }, () => {
  assert.equal(needsLinger(remote('service', { LINGER: 'no', ENABLE_EXIT: '1' }).stdout), true)
  assert.equal(needsLinger(remote('service', { LINGER: 'no', ENABLE_EXIT: '0' }).stdout), false)
  // No linger on macOS: launchd keeps the agent.
  assert.equal(needsLinger(remote('service', { OS: 'Darwin', LINGER: 'no' }).stdout), false)
})

test('a failed service install fails the step without a linger hint', { skip }, () => {
  const r = remote('service', { HOST_EXIT: '1' })
  assert.notEqual(r.status, 0)
  assert.equal(needsLinger(r.stdout), false)
})

test('install steps run in order over the same hardened ssh options as the info command', () => {
  assert.deepEqual(installSteps.map((s) => s.kind), ['download', 'service'])
  const profile: SSHProfile = { id: 'p', host: 'vps', user: 'me', port: 2222, identityFile: null, remotePort: 19333, computerId: null, name: '' }
  const args = installArguments(profile, 'service', '/home/me', '/home/me/.codync')
  assert.deepEqual(args.slice(-3), ['--', 'vps', remoteInstallCommand('service', 19333)])
  for (const option of ['BatchMode=yes', 'ForwardAgent=no', 'StrictHostKeyChecking=yes']) assert.ok(args.includes(option), option)
  assert.deepEqual(args.slice(args.indexOf('-p'), args.indexOf('-p') + 2), ['-p', '2222'])
})

test('the card shows the last lines of errors, without colors or progress redraws', () => {
  const stderr = '######   12.0%\r########## 100.0%\n\x1b[31merror:\x1b[0m checksum mismatch for codync-host-linux-arm64.tar.gz\n'
  assert.equal(installOutput('\x1b[1m==>\x1b[0m Downloading', stderr, 2), '########## 100.0%\nerror: checksum mismatch for codync-host-linux-arm64.tar.gz')
  // Nothing on stderr: what it printed instead.
  assert.equal(installOutput('a\nb\nc\n', '', 2), 'b\nc')
  assert.equal(installOutput('', ''), '')
  assert.equal(needsLinger(`Tip: \`loginctl enable-linger $USER\` keeps the host running\n${LINGER_NEEDED}\n`), true)
  assert.equal(needsLinger('Tip: `loginctl enable-linger $USER` keeps the host running\n'), false)
})
