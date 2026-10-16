import { spawn, type ChildProcess } from 'node:child_process'
import { existsSync, promises as fs } from 'node:fs'
import { win32 } from 'node:path'
import { knownHostsFiles, orphanTunnelPattern } from './ssh-command.ts'

export function sshTools(os = process.platform, systemRoot = process.env.SystemRoot ?? 'C:\\Windows') {
  const directory = os === 'win32' ? win32.join(systemRoot, 'System32', 'OpenSSH') : '/usr/bin'
  const tool = (name: string) => os === 'win32' ? win32.join(directory, `${name}.exe`) : `${directory}/${name}`
  return { ssh: tool('ssh'), keygen: tool('ssh-keygen'), keyscan: tool('ssh-keyscan') }
}

const tools = sshTools()
export const SSH = tools.ssh
export const KEYGEN = tools.keygen
export const KEYSCAN = tools.keyscan
const POWERSHELL = win32.join(process.env.SystemRoot ?? 'C:\\Windows', 'System32', 'WindowsPowerShell', 'v1.0', 'powershell.exe')
const LSOF = ['/usr/sbin/lsof', '/usr/bin/lsof'].find((p) => existsSync(p)) ?? null
export const lastLine = (text: string) => text.split(/\r?\n/).filter(Boolean).pop() ?? ''

interface Result { status: number; stdout: string; stderr: string }

/** Runs a tool with hidden Windows consoles while draining both output pipes. */
export function run(bin: string, args: string[], input?: string): Promise<Result> {
  return new Promise((resolve) => {
    let stdout = ''
    let stderr = ''
    let child: ChildProcess
    try {
      child = spawn(bin, args, { stdio: [input === undefined ? 'ignore' : 'pipe', 'pipe', 'pipe'], windowsHide: true })
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

function powershell(script: string) {
  return run(POWERSHELL, ['-NoProfile', '-NonInteractive', '-EncodedCommand', Buffer.from(script, 'utf16le').toString('base64')])
}

export function listenerCommand(pid: number, port: number) {
  if (!Number.isSafeInteger(pid) || pid <= 0 || !Number.isInteger(port) || port < 1 || port > 65535) throw new Error('Invalid listener')
  return `Get-NetTCPConnection -LocalAddress 127.0.0.1 -LocalPort ${port} -State Listen -ErrorAction SilentlyContinue | Where-Object { $_.OwningProcess -eq ${pid} } | ForEach-Object { $_.OwningProcess }`
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

interface WindowsProcess { ProcessId: number; ExecutablePath: string | null; CommandLine: string | null }

/** Match only native ssh tunnels carrying this variant's exact private known_hosts file. */
export function ownsWindowsTunnel(process: WindowsProcess, binary: string, knownHosts: string) {
  if (!process.ExecutablePath || !process.CommandLine || !Number.isSafeInteger(process.ProcessId) || process.ProcessId <= 0) return false
  if (win32.normalize(process.ExecutablePath).toLowerCase() !== win32.normalize(binary).toLowerCase()) return false
  const command = process.CommandLine
  const marker = knownHosts.replaceAll('\\', '/')
  return /\s-N\s+-T\s/.test(command) && command.includes('UserKnownHostsFile=') &&
    (command.includes(`\\"${marker}\\"`) || command.includes(`"${marker}"`)) &&
    /\s-L\s+127\.0\.0\.1:\d+:127\.0\.0\.1:\d+(?:\s|$)/.test(command)
}

export async function killOrphanTunnels(home: string, dir: string) {
  if (process.platform !== 'win32') {
    await run('/usr/bin/pkill', ['-f', '--', orphanTunnelPattern(dir)])
    return
  }
  const result = await powershell(`$ErrorActionPreference='Stop'; $p = @(Get-CimInstance Win32_Process -Filter "Name='ssh.exe'" | Select-Object ProcessId, ExecutablePath, CommandLine); ConvertTo-Json -InputObject $p -Compress -Depth 2`)
  if (result.status !== 0) { console.error('ssh: could not inspect leftover tunnels'); return }
  try {
    const processes = JSON.parse(result.stdout) as WindowsProcess[]
    const marker = knownHostsFiles(home, dir)[1]!
    for (const process of processes) {
      if (!ownsWindowsTunnel(process, SSH, marker)) continue
      try { globalThis.process.kill(process.ProcessId) } catch { /* Already exited. */ }
    }
  } catch (error) { console.error('ssh: could not inspect leftover tunnels', error) }
}
