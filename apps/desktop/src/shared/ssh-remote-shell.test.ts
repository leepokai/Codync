import assert from 'node:assert/strict'
import { test } from 'node:test'
import { REMOTE_PROBE, infoArguments, probeArguments, remoteInfoCommand, remoteShell, tunnelArguments } from '../main/ssh-command.ts'
import { isWindowsShell, type RemoteShell, type SSHProfile } from './ssh.ts'

const profile: SSHProfile = {
  id: 'p', host: 'alexpc', user: 'bralg', port: null, identityFile: null,
  remotePort: 19222, computerId: null, name: 'Alex PC',
}

// What each default shell answered the probe, taken from real hosts.
const answers = {
  // Windows' cmd.exe expands %OS% and leaves PowerShell's variable alone.
  cmd: 'Windows_NT $env:OS\r\n',
  // PowerShell's echo prints each argument on its own line, so the expansion comes second.
  powershell: '%OS%\r\nWindows_NT\r\n',
  posix: '%OS% :OS\n',
} satisfies Record<RemoteShell, string>

for (const [shell, stdout] of Object.entries(answers)) {
  test(`one probe tells a ${shell} host from the others`, () => {
    assert.equal(remoteShell(stdout), shell)
  })
}

test('a host that answers nothing is asked for its identity as a POSIX host', () => {
  assert.equal(remoteShell(''), 'posix')
  assert.equal(remoteInfoCommand(19222, '.codync', remoteShell('')), remoteInfoCommand(19222))
})

test('the probe runs over the same hardened ssh options as the identity command', () => {
  const args = probeArguments(profile, '/Users/me', '/Users/me/.codync')
  assert.deepEqual(args.slice(-3), ['--', 'alexpc', REMOTE_PROBE])
  for (const option of ['BatchMode=yes', 'ForwardAgent=no', 'StrictHostKeyChecking=yes']) assert.ok(args.includes(option), option)
  // Nothing of the remote shell's own syntax: the same string has to survive cmd.exe, PowerShell and sh.
  assert.doesNotMatch(REMOTE_PROBE, /["'`;&|]/)
})

test('a Windows host reports its identity through the host the desktop app installed', () => {
  for (const shell of ['cmd', 'powershell'] as const) {
    const command = remoteInfoCommand(19444, '.codync', shell)
    assert.match(command, /Programs\\codync-desktop\\resources/)
    assert.match(command, /info --json --port 19444/)
    // The folder comes after the computer's own PATH, as on a Mac or Linux computer.
    assert.ok(command.indexOf('PATH') < command.indexOf('Programs'), command)
    // A missing host exits 127 there too, which is what shows the "not installed" card.
    assert.match(command, /exit 127/)
    assert.doesNotMatch(command, /sh -lc/)
  }
})

test('the PowerShell command survives the quotes sshd puts around it', () => {
  // sshd runs `powershell -c "<command>"`, so a double quote of ours would end the command early.
  assert.doesNotMatch(remoteInfoCommand(19222, '.codync', 'powershell'), /"/)
  assert.doesNotMatch(remoteInfoCommand(19223, '.codync-dev', 'powershell'), /"/)
  // PowerShell exits 0 for a failed native command unless the script passes its code on.
  assert.match(remoteInfoCommand(19222, '.codync', 'powershell'), /exit \$LASTEXITCODE$/)
})

test('development on Windows uses only its own host and data folder', () => {
  for (const shell of ['cmd', 'powershell'] as const) {
    const command = remoteInfoCommand(19223, '.codync-dev', shell)
    assert.match(command, /Programs\\codync-desktop-dev\\resources\\codync-host\.exe/)
    assert.match(command, /CODYNC_HOME/)
    assert.match(command, /\.codync-dev/)
    assert.match(command, /info --json --port 19223/)
    // Never the production host: its data folder is a different computer's.
    assert.doesNotMatch(command, /Programs\\codync-desktop\\resources/)
  }
})

test('a Mac or Linux host keeps the login shell command it always had', () => {
  assert.match(remoteInfoCommand(19222, '.codync', 'posix'), /^sh -lc 'PATH=/)
  assert.equal(infoArguments(profile, '/Users/me').at(-1), remoteInfoCommand(19222))
  assert.equal(infoArguments(profile, '/Users/me', '.codync', undefined, 'cmd').at(-1), remoteInfoCommand(19222, '.codync', 'cmd'))
})

test('the tunnel to a Windows host is the same port forward as to any other', () => {
  const args = tunnelArguments(profile, 19333, '/Users/me', '/Users/me/.codync')
  // No remote command runs in the tunnel, so no remote shell is involved: the forward is ssh's own work.
  assert.deepEqual(args.slice(-2), ['--', 'alexpc'])
  assert.ok(args.includes('-L') && args.includes('127.0.0.1:19333:127.0.0.1:19222'))
  assert.ok(args.includes('-N') && args.includes('ExitOnForwardFailure=yes'))
})

test('Codync installs the host itself only where it has an installer to run', () => {
  assert.equal(isWindowsShell('cmd'), true)
  assert.equal(isWindowsShell('powershell'), true)
  assert.equal(isWindowsShell('posix'), false)
  // Before the probe answers, the card keeps the install it has always offered.
  assert.equal(isWindowsShell(undefined), false)
})
