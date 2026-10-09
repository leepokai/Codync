import { spawn, type ChildProcess } from 'node:child_process'
import { createHash } from 'node:crypto'
import { existsSync, promises as fs, readFileSync, statSync } from 'node:fs'
import { createServer } from 'node:net'
import { homedir, platform } from 'node:os'
import { join } from 'node:path'
import { app, dialog, ipcMain, type BrowserWindow } from 'electron'
import type { SSHAttachment, SSHProfile, SSHState, SSHStatus } from '../shared/ssh'
import { infoArguments, knownHostsFiles, resolveArguments, tunnelArguments } from './ssh-command'

// SSH computers (port of the Mac app's SSHTunnel.swift): profiles, host key checks and one
// OpenSSH tunnel per connected profile. Arguments are always an argv array, never a shell string.

const SSH = '/usr/bin/ssh'
const KEYGEN = '/usr/bin/ssh-keygen'
const KEYSCAN = '/usr/bin/ssh-keyscan'
const LSOF = ['/usr/sbin/lsof', '/usr/bin/lsof'].find((p) => existsSync(p)) ?? null
const home = homedir()

// MARK: pure helpers

/** A reason the profile can't be used, or null. */
export function validate(p: SSHProfile): string | null {
  if (p.host.startsWith('-') || !/^[A-Za-z0-9._-]+$/.test(p.host)) {
    return 'Use an SSH alias or a hostname: letters, digits, dots, dashes and underscores.'
  }
  if (p.user !== null && !/^[A-Za-z_][A-Za-z0-9._-]*$/.test(p.user)) return "That user name isn't valid."
  const inRange = (n: number) => Number.isInteger(n) && n >= 1 && n <= 65535
  if (p.port !== null && !inRange(p.port)) return 'The SSH port must be between 1 and 65535.'
  if (!inRange(p.remotePort)) return 'The Codync port must be between 1 and 65535.'
  if (p.identityFile !== null) {
    let ok = false
    try {
      ok = p.identityFile.startsWith('/') && statSync(p.identityFile).isFile()
    } catch {}
    if (!ok) return "The key file doesn't exist."
  }
  return null
}

/** The far side rejected our key (BatchMode never prompts, so a password or passphrase can't help). */
export const authRefused = (stderr: string) => stderr.includes('Permission denied') || stderr.includes('Too many authentication failures')

/** What `ssh -G` resolved the destination to. */
export interface Resolved {
  hostname: string
  port: number
  hostKeyAlias: string | null
  proxyJump: string | null
  proxyCommand: string | null
}

const usesProxy = (r: Resolved) => r.proxyJump !== null || r.proxyCommand !== null
/** The name ssh looks up in known_hosts. */
export const knownHostsName = (r: Resolved) => {
  const name = r.hostKeyAlias ?? r.hostname
  return r.port === 22 ? name : `[${name}]:${r.port}`
}

export function parseConfig(text: string): Resolved | null {
  const values = new Map<string, string>()
  for (const line of text.split(/\r?\n/)) {
    const at = line.indexOf(' ')
    if (at < 0) continue
    const key = line.slice(0, at).toLowerCase()
    if (!values.has(key)) values.set(key, line.slice(at + 1).trim())
  }
  const set = (k: string) => {
    const v = values.get(k)
    return v && v !== 'none' ? v : null
  }
  const hostname = set('hostname')
  const port = Number(values.get('port'))
  if (!hostname || !Number.isInteger(port) || !values.get('port')) return null
  return { hostname, port, hostKeyAlias: set('hostkeyalias'), proxyJump: set('proxyjump'), proxyCommand: set('proxycommand') }
}

const keyscanArguments = (r: Resolved) => ['-T', '5', '-p', String(r.port), '--', r.hostname]

/** `type key` of each host key line (known_hosts or ssh-keyscan output); comments and revoked keys skipped. */
export function hostKeys(text: string, certAuthorities = true): Set<string> {
  const keys = new Set<string>()
  for (const line of text.split(/\r?\n/)) {
    const fields = line.split(/\s+/).filter(Boolean)
    const first = fields[0]
    if (!first || first.startsWith('#')) continue
    if (first.startsWith('@')) {
      if (!certAuthorities || first !== '@cert-authority') continue
      fields.shift()
    }
    if (fields.length >= 3) keys.add(`${fields[1]} ${fields[2]}`)
  }
  return keys
}

/** ssh-keyscan's lines, filed under the name ssh will look up. */
export const knownHostsLines = (scan: string, name: string) => [...hostKeys(scan)].sort().map((k) => `${name} ${k}`)

/** A computer id is the first 16 bytes of SHA-256 of its signing key, base64url. */
function consistent(id: string, signKey: string) {
  const key = Buffer.from(signKey, 'base64url')
  return key.length === 32 && createHash('sha256').update(key).digest().subarray(0, 16).toString('base64url') === id
}

// MARK: processes

interface Result {
  status: number
  stdout: string
  stderr: string
}

/** The last thing the tool said, for showing next to an error. */
const lastLine = (text: string) => text.split(/\r?\n/).filter(Boolean).pop() ?? ''

/** Runs a tool to completion; both pipes are drained while it runs. */
function run(bin: string, args: string[], input?: string): Promise<Result> {
  return new Promise((resolve) => {
    let stdout = ''
    let stderr = ''
    let child: ChildProcess
    try {
      child = spawn(bin, args, { stdio: [input === undefined ? 'ignore' : 'pipe', 'pipe', 'pipe'] })
    } catch {
      return resolve({ status: -1, stdout, stderr })
    }
    child.stdout?.on('data', (d: Buffer) => (stdout += d.toString()))
    child.stderr?.on('data', (d: Buffer) => (stderr += d.toString()))
    child.on('error', () => resolve({ status: -1, stdout, stderr }))
    child.on('close', (code) => resolve({ status: code ?? -1, stdout, stderr }))
    if (input !== undefined) child.stdin?.end(input)
  })
}

/** Tunnels a crashed or force-quit Codync left running: only Codync's tunnels carry this known_hosts file. */
const killOrphanTunnels = () => run('/usr/bin/pkill', ['-f', '--', '^/usr/bin/ssh -N .*\\.codync/ssh_known_hosts'])

/** Binds port 0 on loopback to learn a free port. */
function freeLocalPort(): Promise<number | null> {
  return new Promise((resolve) => {
    const server = createServer()
    server.once('error', () => resolve(null))
    server.listen(0, '127.0.0.1', () => {
      const address = server.address()
      server.close(() => resolve(typeof address === 'object' && address ? address.port : null))
    })
  })
}

/** Whether process `pid` holds the listening socket on 127.0.0.1:`port`. */
async function sshListens(pid: number, port: number): Promise<boolean> {
  if (LSOF) {
    const found = await run(LSOF, ['-a', '-n', '-P', '-p', String(pid), `-iTCP@127.0.0.1:${port}`, '-sTCP:LISTEN', '-t'])
    return found.status === 0 && found.stdout.split(/\r?\n/).includes(String(pid))
  }
  if (platform() !== 'linux') return false
  // No lsof: the listening socket's inode from /proc/net/tcp, then whether `pid` holds it.
  try {
    const local = `0100007F:${port.toString(16).toUpperCase().padStart(4, '0')}`
    const inodes = (await fs.readFile('/proc/net/tcp', 'utf8'))
      .split('\n')
      .map((l) => l.trim().split(/\s+/))
      .filter((f) => f[1] === local && f[3] === '0A')
      .map((f) => `socket:[${f[9]}]`)
    if (inodes.length === 0) return false
    for (const fd of await fs.readdir(`/proc/${pid}/fd`)) {
      const link = await fs.readlink(`/proc/${pid}/fd/${fd}`).catch(() => '')
      if (inodes.includes(link)) return true
    }
  } catch {}
  return false
}

async function healthOf(baseURL: string): Promise<{ computerId?: string } | null> {
  try {
    const res = await fetch(`${baseURL}/health`, { signal: AbortSignal.timeout(1000) })
    return res.ok ? ((await res.json()) as { computerId?: string }) : null
  } catch {
    return null
  }
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))

/** What `codync-host info --json` prints. */
interface RemoteInfo {
  name: string
  computerId: string
  signKey: string
  boxKey?: string | null
  token: string
  running: boolean
}

function parseInfo(text: string): RemoteInfo | null {
  try {
    const v = JSON.parse(text) as Partial<RemoteInfo>
    return typeof v.name === 'string' && typeof v.computerId === 'string' && typeof v.signKey === 'string' && typeof v.token === 'string' && typeof v.running === 'boolean'
      ? (v as RemoteInfo)
      : null
  } catch {
    return null
  }
}

// MARK: the manager

type Outcome = { kind: 'attached' } | { kind: 'stop'; status: SSHStatus } | { kind: 'retry'; message: string }
interface Token {
  cancelled: boolean
}

const signInRefused = (p: SSHProfile) =>
  `${p.host} refused the key. Codync signs in with an SSH key (ssh-agent or the key file) and can't type a password or passphrase; make \`ssh ${p.host}\` work in Terminal without prompting, then connect again.`

class SSHComputers {
  profiles: SSHProfile[] = []
  private status = new Map<string, SSHStatus>()
  private attachments = new Map<string, SSHAttachment>()
  private tunnels = new Map<string, ChildProcess>()
  private tasks = new Map<string, Token>()
  private backoff = new Map<string, number>()
  private connectedAt = new Map<string, number>()
  /** Host key lines waiting for the user's "Trust and connect". */
  private pendingKeys = new Map<string, string[]>()

  constructor(private file: string, private emit: (s: SSHState) => void) {
    try {
      this.profiles = JSON.parse(readFileSync(file, 'utf8')) as SSHProfile[]
    } catch {}
  }

  get state(): SSHState {
    return { profiles: this.profiles, status: Object.fromEntries(this.status), attachments: [...this.attachments.values()] }
  }

  private changed() {
    this.emit(this.state)
  }

  private setStatus(id: string, s: SSHStatus) {
    this.status.set(id, s)
    this.changed()
  }

  async connectAll() {
    // Before this copy has tunnels of its own, so only leftovers from an earlier copy match.
    if (this.tunnels.size === 0 && this.tasks.size === 0) await killOrphanTunnels()
    for (const p of this.profiles) if (!this.tasks.has(p.id) && !this.tunnels.has(p.id)) this.connect(p.id)
  }

  save(profile: SSHProfile): string | null {
    const old = this.profiles.find((p) => p.id === profile.id)
    // A different address may be a different computer: learn its identity again.
    const p = old && (old.host !== profile.host || old.port !== profile.port) ? { ...profile, computerId: null } : profile
    const problem = validate(p)
    if (problem) return problem
    if (old) {
      this.disconnect(p.id)
      this.profiles = this.profiles.map((x) => (x.id === p.id ? p : x))
    } else {
      this.profiles = [...this.profiles, p]
    }
    this.persist()
    this.connect(p.id)
    return null
  }

  remove(id: string) {
    this.disconnect(id)
    this.profiles = this.profiles.filter((p) => p.id !== id)
    this.status.delete(id)
    this.backoff.delete(id)
    this.persist()
    this.changed()
  }

  connect(id: string) {
    const old = this.tasks.get(id)
    if (old) old.cancelled = true
    const token: Token = { cancelled: false }
    this.tasks.set(id, token)
    void this.run(id, token)
  }

  /** Ends the tunnel for this session (bots on it go away until the next connect). */
  disconnect(id: string) {
    const task = this.tasks.get(id)
    if (task) task.cancelled = true
    this.tasks.delete(id)
    this.stopTunnel(id)
    this.setStatus(id, { kind: 'idle' })
  }

  disconnectAll() {
    for (const id of new Set([...this.tunnels.keys(), ...this.tasks.keys()])) this.disconnect(id)
  }

  /** The user compared the fingerprints: remember the key in Codync's known_hosts and go on. */
  async trustHostKey(id: string) {
    const lines = this.pendingKeys.get(id)
    if (this.status.get(id)?.kind !== 'confirmHostKey' || !lines) return
    try {
      const dir = join(home, '.codync')
      await fs.mkdir(dir, { recursive: true, mode: 0o700 })
      await fs.appendFile(join(dir, 'ssh_known_hosts'), lines.join('\n') + '\n', { mode: 0o600 })
    } catch (e) {
      return this.setStatus(id, { kind: 'failed', message: `Couldn't save the host key: ${e instanceof Error ? e.message : String(e)}` })
    }
    this.pendingKeys.delete(id)
    this.connect(id)
  }

  // MARK: connecting

  private async run(id: string, token: Token) {
    while (!token.cancelled) {
      const profile = this.profiles.find((p) => p.id === id)
      if (!profile) return
      const outcome = await this.attach(profile, token)
      // Disconnected or replaced meanwhile: the newer task owns the state now.
      if (token.cancelled) return
      switch (outcome.kind) {
        case 'attached':
          this.backoff.delete(id)
          this.tasks.delete(id)
          return
        case 'stop':
          this.tasks.delete(id)
          return this.setStatus(id, outcome.status)
        case 'retry': {
          const delay = this.backoff.get(id) ?? 1
          this.backoff.set(id, Math.min(delay * 2, 30))
          this.setStatus(id, { kind: 'retrying', message: outcome.message })
          // Full jitter, like the device relay backoff.
          await sleep(1000 * (0.5 + Math.random() * Math.max(0, delay - 0.5)))
        }
      }
    }
  }

  private async attach(profile: SSHProfile, token: Token): Promise<Outcome> {
    const id = profile.id
    const stop = (status: SSHStatus): Outcome => ({ kind: 'stop', status })
    const idle = stop({ kind: 'idle' })
    const problem = validate(profile)
    if (problem) return stop({ kind: 'failed', message: problem })
    this.setStatus(id, { kind: 'connecting', step: 'Reading SSH settings…' })
    const config = await run(SSH, resolveArguments(profile))
    if (token.cancelled) return idle
    const resolved = config.status === 0 ? parseConfig(config.stdout) : null
    if (!resolved) return stop({ kind: 'failed', message: `SSH couldn't read the settings for ${profile.host}. ${lastLine(config.stderr)}` })
    const name = knownHostsName(resolved)

    // Host key (never by reading ssh's error text).
    if ((await this.recordedKeys(name)).size === 0) {
      if (usesProxy(resolved)) {
        return stop({ kind: 'failed', message: `${profile.host} goes through a jump host. Connect once with ssh in Terminal to confirm its host key, then try again.` })
      }
      this.setStatus(id, { kind: 'connecting', step: 'Reading the host key…' })
      const scan = await run(KEYSCAN, keyscanArguments(resolved))
      const lines = knownHostsLines(scan.stdout, name)
      if (lines.length === 0) return { kind: 'retry', message: `Can't reach ${resolved.hostname}:${resolved.port}.` }
      const prints = await run(KEYGEN, ['-l', '-f', '-'], lines.join('\n') + '\n')
      const fingerprints = prints.stdout.split(/\r?\n/).filter(Boolean)
      if (prints.status !== 0 || fingerprints.length === 0) return stop({ kind: 'failed', message: `Couldn't read the host key of ${profile.host}.` })
      this.pendingKeys.set(id, lines)
      return stop({ kind: 'confirmHostKey', name, fingerprints })
    }

    this.setStatus(id, { kind: 'connecting', step: 'Signing in…' })
    const result = await run(SSH, infoArguments(profile, home))
    if (token.cancelled) return idle
    if (result.status === 255) {
      if (authRefused(result.stderr)) return stop({ kind: 'failed', message: signInRefused(profile) })
      if (!usesProxy(resolved)) {
        const current = hostKeys((await run(KEYSCAN, keyscanArguments(resolved))).stdout)
        // Only plain host keys can be compared: a `@cert-authority` key is never what keyscan returns.
        const pinned = await this.recordedKeys(name, false)
        if (current.size > 0 && pinned.size > 0 && ![...current].some((k) => pinned.has(k))) {
          return stop({ kind: 'failed', message: `The host key of ${profile.host} changed. This can mean someone is intercepting the connection. Codync won't connect until the old key is removed from known_hosts.` })
        }
      }
      return { kind: 'retry', message: `SSH couldn't connect to ${profile.host}. ${lastLine(result.stderr)}` }
    }
    if (result.status === 127) return stop({ kind: 'notInstalled' })
    const info = parseInfo(result.stdout)
    if (!info) {
      if (result.status === 0) return stop({ kind: 'notInstalled' })
      return stop({ kind: 'failed', message: `codync-host on ${profile.host} couldn't report its identity. Run \`codync-host install\` there. ${lastLine(result.stderr)}` })
    }
    if (!consistent(info.computerId, info.signKey)) return stop({ kind: 'failed', message: `${profile.host} reported an invalid identity.` })
    if (profile.computerId && profile.computerId !== info.computerId) {
      return stop({ kind: 'failed', message: `${profile.host} is now a different computer than the one saved. If it was reinstalled, remove it and add it again.` })
    }
    if (!profile.computerId) this.remember(id, info.computerId, info.name)
    if (!info.running) return { kind: 'retry', message: `codync-host is installed on ${info.name} but not running. Run \`codync-host install\` there.` }

    // Tunnel: a free loopback port each attempt; a port taken in between just makes ssh exit (ExitOnForwardFailure).
    let lastProblem = ''
    for (let attempt = 0; attempt < 3; attempt++) {
      const port = await freeLocalPort()
      if (port === null) return { kind: 'retry', message: 'No free local port for the tunnel.' }
      this.setStatus(id, { kind: 'connecting', step: 'Opening the tunnel…' })
      const tunnel = this.startTunnel(profile, port)
      if (!tunnel) return stop({ kind: 'failed', message: "Couldn't start ssh." })
      const { child, tail } = tunnel
      const alive = () => child.exitCode === null && child.signalCode === null
      const baseURL = `http://127.0.0.1:${port}`
      let health: { computerId?: string } | null = null
      for (let i = 0; i < 60 && alive() && !token.cancelled; i++) {
        health = await healthOf(baseURL)
        if (health) break
        await sleep(250)
      }
      if (token.cancelled) {
        child.kill()
        return idle
      }
      if (!health) {
        if (alive()) child.kill()
        else lastProblem = await tail.lastLine()
        continue
      }
      if (health.computerId !== info.computerId) {
        child.kill()
        return stop({ kind: 'failed', message: `The tunnel to ${profile.host} reached a different computer than expected.` })
      }
      // /health is public, so the answer alone doesn't prove it came through ssh: another
      // local user could have taken the port. Only a listener ssh owns gets the token.
      const listens = child.pid !== undefined && (await sshListens(child.pid, port))
      // Disconnected, removed or edited meanwhile: this attempt must not register its tunnel.
      if (token.cancelled) {
        child.kill()
        return idle
      }
      if (!listens || !alive()) {
        console.error(`ssh: tunnel port ${port} answered by something other than ssh`)
        child.kill()
        lastProblem = 'Another program answered on the tunnel\'s local port.'
        continue
      }
      this.watch(child, id, tail)
      this.tunnels.set(id, child)
      this.connectedAt.set(id, Date.now())
      this.attachments.set(id, {
        profileId: id,
        computer: { id: info.computerId, name: info.name, signKey: info.signKey, boxKey: info.boxKey ?? null },
        baseURL,
        token: info.token,
      })
      this.setStatus(id, { kind: 'connected', localPort: port })
      return { kind: 'attached' }
    }
    if (authRefused(lastProblem)) return stop({ kind: 'failed', message: signInRefused(profile) })
    return { kind: 'retry', message: `Couldn't open the tunnel to ${profile.host}. ${lastProblem}` }
  }

  private async recordedKeys(name: string, certAuthorities = true) {
    const keys = new Set<string>()
    for (const file of knownHostsFiles(home)) {
      if (!existsSync(file)) continue
      const found = await run(KEYGEN, ['-F', name, '-f', file])
      if (found.status === 0) for (const k of hostKeys(found.stdout, certAuthorities)) keys.add(k)
    }
    return keys
  }

  /** ssh's stderr is read as it arrives: left unread, the pipe fills and ssh blocks mid-write, freezing the tunnel. */
  private startTunnel(profile: SSHProfile, localPort: number) {
    let text = ''
    let done = false
    let child: ChildProcess
    try {
      child = spawn(SSH, tunnelArguments(profile, localPort, home), { stdio: ['ignore', 'ignore', 'pipe'] })
    } catch {
      return null
    }
    child.on('error', () => {})
    child.stderr?.on('data', (d: Buffer) => (text = (text + d.toString()).slice(-4096)))
    child.stderr?.on('close', () => (done = true))
    const tail = {
      /** ssh's last line, once it exited (waits briefly for the pipe to reach its end). */
      async lastLine() {
        for (let i = 0; i < 40 && !done; i++) await sleep(25)
        return lastLine(text)
      },
    }
    return { child, tail }
  }

  /** A tunnel that drops is reopened with backoff (reset once it stayed up a minute). */
  private watch(child: ChildProcess, id: string, tail: { lastLine(): Promise<string> }) {
    child.once('exit', () => {
      void tail.lastLine().then((detail) => {
        // Only the current tunnel: a replaced or disconnected one ends without effect.
        if (this.tunnels.get(id) !== child) return
        console.log(`ssh tunnel ended: ${detail}`)
        const since = this.connectedAt.get(id)
        if (since !== undefined && Date.now() - since >= 60_000) this.backoff.delete(id)
        this.stopTunnel(id)
        this.setStatus(id, { kind: 'retrying', message: `The SSH connection dropped. ${detail}` })
        this.connect(id)
      })
    })
  }

  private stopTunnel(id: string) {
    const child = this.tunnels.get(id)
    this.tunnels.delete(id)
    child?.kill()
    this.connectedAt.delete(id)
    if (this.attachments.delete(id)) this.changed()
  }

  private remember(id: string, computerId: string, name: string) {
    this.profiles = this.profiles.map((p) => (p.id === id ? { ...p, computerId, name: p.name || name } : p))
    this.persist()
    this.changed()
  }

  private persist() {
    void fs.mkdir(app.getPath('userData'), { recursive: true }).then(() => fs.writeFile(this.file, JSON.stringify(this.profiles)))
  }
}

/** SSH computers for every window: `ssh:*` handlers, `ssh:change` events, tunnels stopped on quit. */
export function registerSSH(getWindows: () => BrowserWindow[]) {
  const ssh = new SSHComputers(join(app.getPath('userData'), 'ssh-profiles.json'), (state) => {
    for (const w of getWindows()) if (!w.isDestroyed()) w.webContents.send('ssh:change', state)
  })
  ipcMain.handle('ssh:state', () => ssh.state)
  ipcMain.handle('ssh:save', (_e, profile: SSHProfile) => ssh.save(profile))
  ipcMain.on('ssh:remove', (_e, id: string) => ssh.remove(id))
  ipcMain.on('ssh:connect', (_e, id: string) => ssh.connect(id))
  ipcMain.on('ssh:disconnect', (_e, id: string) => ssh.disconnect(id))
  ipcMain.on('ssh:trustHostKey', (_e, id: string) => void ssh.trustHostKey(id))
  ipcMain.handle('ssh:chooseKey', async () => {
    const result = await dialog.showOpenDialog({ defaultPath: join(home, '.ssh'), properties: ['openFile', 'showHiddenFiles'] })
    return result.canceled ? null : (result.filePaths[0] ?? null)
  })
  // ssh children outlive the app unless stopped.
  app.on('will-quit', () => ssh.disconnectAll())
  void ssh.connectAll()
}
