import { spawn, type ChildProcess } from 'node:child_process'
import { existsSync, promises as fs } from 'node:fs'
import { tmpdir } from 'node:os'
import { join, win32 } from 'node:path'
import { keyscanArguments, orphanTunnelPattern, ownsWindowsTunnel, presentedKeyArguments, type WindowsProcess } from './ssh-command.ts'
import type { Resolved } from './ssh-parse.ts'

const systemRoot = process.env.SystemRoot ?? 'C:\\Windows'

export function sshTools(os = process.platform, root = systemRoot) {
  const directory = os === 'win32' ? win32.join(root, 'System32', 'OpenSSH') : '/usr/bin'
  const tool = (name: string) => os === 'win32' ? win32.join(directory, `${name}.exe`) : `${directory}/${name}`
  return { ssh: tool('ssh'), keygen: tool('ssh-keygen'), keyscan: tool('ssh-keyscan') }
}

const tools = sshTools()
export const SSH = tools.ssh
export const KEYGEN = tools.keygen
export const KEYSCAN = tools.keyscan
const POWERSHELL = win32.join(systemRoot, 'System32', 'WindowsPowerShell', 'v1.0', 'powershell.exe')
const LSOF = ['/usr/sbin/lsof', '/usr/bin/lsof'].find((p) => existsSync(p)) ?? null

/** How to install the OpenSSH tools when one is missing, or null. */
export function missingTools(): string | null {
  if ([SSH, KEYGEN, KEYSCAN].every((tool) => existsSync(tool))) return null
  return process.platform === 'win32' ? 'Install OpenSSH Client in Windows Settings → Optional features, then connect again.' : 'Install OpenSSH, then connect again.'
}

/** The last thing the tool said, for showing next to an error. */
export const lastLine = (text: string) => text.split(/\r?\n/).filter(Boolean).pop() ?? ''

interface Result { status: number; stdout: string; stderr: string }

/** Runs a tool with hidden Windows consoles while draining both output pipes; one that hangs is killed after 30 s. */
export function run(bin: string, args: string[], input?: string): Promise<Result> {
  return new Promise((resolve) => {
    let stdout = ''
    let stderr = ''
    let child: ChildProcess
    try {
      child = spawn(bin, args, { stdio: [input === undefined ? 'ignore' : 'pipe', 'pipe', 'pipe'], windowsHide: true, timeout: 30_000 })
    } catch (error) {
      return resolve({ status: -1, stdout, stderr: String(error) })
    }
    child.stdout?.on('data', (d: Buffer) => (stdout += d.toString()))
    child.stderr?.on('data', (d: Buffer) => (stderr += d.toString()))
    child.once('error', (error) => { resolve({ status: -1, stdout, stderr: error.message }) })
    child.once('close', (code) => { resolve({ status: code ?? -1, stdout, stderr }) })
    child.stdin?.on('error', () => { /* A tool can exit before consuming its input. */ })
    if (input !== undefined) child.stdin?.end(input)
  })
}

/**
 * The host's keys in known_hosts form, read before any sign-in. Windows' ssh-keyscan (OpenSSH 9.5) offers a key exchange
 * it can't run and reads nothing from current servers, so there ssh records the one key the host presents instead.
 */
export async function scanHostKeys(r: Resolved): Promise<string> {
  if (process.platform !== 'win32') return (await run(KEYSCAN, keyscanArguments(r))).stdout
  const dir = await fs.mkdtemp(join(tmpdir(), 'codync-ssh-')).catch(() => null)
  if (dir === null) return ''
  const file = join(dir, 'known_hosts')
  await run(SSH, presentedKeyArguments(r, file))
  const text = await fs.readFile(file, 'utf8').catch(() => '')
  await fs.rm(dir, { recursive: true, force: true }).catch(() => { /* A scratch folder left in temp is harmless. */ })
  return text
}

function powershell(script: string) {
  return run(POWERSHELL, ['-NoProfile', '-NonInteractive', '-EncodedCommand', Buffer.from(script, 'utf16le').toString('base64')])
}

export function listenerCommand(pid: number, port: number) {
  if (!Number.isSafeInteger(pid) || pid <= 0 || !Number.isInteger(port) || port < 1 || port > 65535) throw new Error('Invalid listener')
  // Some Windows builds misreport listening sockets' State. Verify the owner of
  // an actual loopback connection using its exact reverse tuple instead.
  return `$ErrorActionPreference='Stop'
$client = [System.Net.Sockets.TcpClient]::new()
try {
  $client.Connect('127.0.0.1', ${port})
  $peer = $client.Client.LocalEndPoint.Port
  Get-NetTCPConnection -LocalAddress 127.0.0.1 -LocalPort ${port} -RemoteAddress 127.0.0.1 -RemotePort $peer -ErrorAction Stop |
    Where-Object { $_.OwningProcess -eq ${pid} } | ForEach-Object { $_.OwningProcess }
} finally { $client.Dispose() }`
}

/** Never send the host token until the listener is owned by the exact ssh child. */
export async function sshListens(pid: number, port: number): Promise<boolean> {
  if (process.platform === 'win32') {
    const found = await powershell(listenerCommand(pid, port))
    return found.status === 0 && found.stdout.split(/\r?\n/).some((line) => line.trim() === String(pid))
  }
  if (LSOF) {
    const found = await run(LSOF, ['-a', '-n', '-P', '-p', String(pid), `-iTCP@127.0.0.1:${port}`, '-sTCP:LISTEN', '-t'])
    return found.status === 0 && found.stdout.split(/\r?\n/).includes(String(pid))
  }
  if (process.platform !== 'linux') return false
  try {
    const local = `0100007F:${port.toString(16).toUpperCase().padStart(4, '0')}`
    const tcp = await fs.readFile('/proc/net/tcp', 'utf8')
    const inodes = tcp.split('\n').map((line) => line.trim().split(/\s+/))
      .filter((f) => f[1] === local && f[3] === '0A').map((f) => `socket:[${f[9]}]`)
    if (inodes.length === 0) return false
    const descriptors = await fs.readdir(`/proc/${pid}/fd`)
    for (const fd of descriptors) {
      const link = await fs.readlink(`/proc/${pid}/fd/${fd}`).catch(() => '')
      if (inodes.includes(link)) return true
    }
  } catch { /* A process can exit while its sockets are inspected. */ }
  return false
}

/** Tunnels a crashed or force-quit copy of this app left running: only they carry `dir`'s known_hosts file. */
export async function killOrphanTunnels(dir: string) {
  if (process.platform !== 'win32') {
    await run('/usr/bin/pkill', ['-f', '--', orphanTunnelPattern(SSH, dir)])
    return
  }
  const result = await powershell(`$ErrorActionPreference='Stop'; $p = @(Get-CimInstance Win32_Process -Filter "Name='ssh.exe'" | Select-Object ProcessId, ExecutablePath, CommandLine); ConvertTo-Json -InputObject $p -Compress -Depth 2`)
  if (result.status !== 0) { console.error('ssh: could not inspect leftover tunnels'); return }
  try {
    for (const candidate of JSON.parse(result.stdout) as WindowsProcess[]) {
      if (!ownsWindowsTunnel(candidate, SSH, dir)) continue
      try { process.kill(candidate.ProcessId) } catch { /* Already exited. */ }
    }
  } catch (error) { console.error('ssh: could not inspect leftover tunnels', error) }
}
