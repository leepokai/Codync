import { identity } from './environment'
import { spawn, type ChildProcess } from 'node:child_process'
import { existsSync, promises as fs, readFileSync } from 'node:fs'
import { createServer } from 'node:net'
import { homedir } from 'node:os'
import { join } from 'node:path'
import { app, dialog, ipcMain, type BrowserWindow } from 'electron'
import { dataDir } from './host-controller'
import { isWindowsShell, type SSHAttachment, type SSHMessage, type SSHProfile, type SSHState, type SSHStatus, sshMessage } from '../shared/ssh'
import { codyncKnownHosts, infoArguments, installArguments, installOutput, installSteps, keyscanArguments, knownHostsFiles, needsLinger, probeArguments, resolveArguments, tunnelArguments } from './ssh-command'
import { authRefused, consistent, hostKeys, knownHostsLines, knownHostsName, parseConfig, parseInfo, usesProxy } from './ssh-parse'
import { KEYGEN, KEYSCAN, SSH, killOrphanTunnels, lastLine, missingTools, run, scanHostKeys, sshListens } from './ssh-process'
import { validate } from './ssh-profile'
import { RemoteShells } from './ssh-shells'

// SSH computers: profiles, host key checks and one
// OpenSSH tunnel per connected profile. Arguments are always an argv array, never a shell string.

const home = homedir()

// MARK: processes

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

async function healthOf(baseURL: string): Promise<{ computerId?: string } | null> {
  try {
    const res = await fetch(`${baseURL}/health`, { signal: AbortSignal.timeout(1000) })
    return res.ok ? ((await res.json()) as { computerId?: string }) : null
  } catch {
    return null
  }
}

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))

// MARK: the manager

type Outcome = { kind: 'attached' } | { kind: 'stop'; status: SSHStatus } | { kind: 'retry'; message: SSHMessage }
interface Token {
  cancelled: boolean
}

const signInRefused = (p: SSHProfile) =>
  sshMessage("{name} refused the key. Codync signs in with an SSH key (ssh-agent or the key file) and can't type a password or passphrase; make `ssh {name}` work in Terminal without prompting, then connect again.", { name: p.host })

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
  /** Installed here, but the host stops at logout until the user enables linger with sudo. */
  private needsLinger = new Set<string>()
  private shells = new RemoteShells((p) => run(SSH, probeArguments(p, home, dataDir)))

  /** `reportStale`: a cancelled attempt reached a status it no longer owns (`ssh_step_after_disconnect`). */
  constructor(private file: string, private emit: (s: SSHState) => void, private reportStale: () => void) {
    try {
      this.profiles = JSON.parse(readFileSync(file, 'utf8')) as SSHProfile[]
    } catch {}
  }

  get state(): SSHState {
    return { profiles: this.profiles, status: Object.fromEntries(this.status), attachments: [...this.attachments.values()], needsLinger: [...this.needsLinger] }
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
    if (this.tunnels.size === 0 && this.tasks.size === 0) await killOrphanTunnels(dataDir)
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
      this.shells.forget(p.id)
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
    this.needsLinger.delete(id)
    this.shells.forget(id)
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
      await fs.mkdir(dataDir, { recursive: true, mode: 0o700 })
      await fs.appendFile(codyncKnownHosts(dataDir), lines.join('\n') + '\n', { mode: 0o600 })
    } catch (e) {
      return this.setStatus(id, { kind: 'failed', message: sshMessage("Couldn't save the host key.", {}, e instanceof Error ? e.message : String(e)) })
    }
    this.pendingKeys.delete(id)
    this.connect(id)
  }

  /**
   * Install on a `notInstalled` card: the released host and its service over SSH, then a connection. Only
   * the production app: there's no released development host.
   */
  async install(id: string) {
    const profile = this.profiles.find((p) => p.id === id)
    if (!profile || this.status.get(id)?.kind !== 'notInstalled' || identity.environment !== 'main') return
    // Windows has no host-only installer: the host comes with the desktop app, which the card says to install there.
    if (isWindowsShell(this.shells.get(id))) return
    const token: Token = { cancelled: false }
    this.tasks.set(id, token)
    this.needsLinger.delete(id)
    for (const step of installSteps) {
      this.setStatus(id, { kind: 'installing', step: step.title })
      const result = await run(SSH, installArguments(profile, step.kind, home, dataDir), undefined, step.timeout)
      // Disconnected meanwhile; the remote step finishes on its own.
      if (token.cancelled) return
      if (result.status !== 0) {
        this.tasks.delete(id)
        return this.setStatus(id, { kind: 'notInstalled', failure: { output: installOutput(result.stdout, result.stderr) } })
      }
      if (needsLinger(result.stdout)) this.needsLinger.add(id)
    }
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
    // A disconnected or replaced attempt still runs to its next check: it never shows a step again, or a
    // disconnected computer would stay on that step.
    const show = (status: SSHStatus) => {
      if (!token.cancelled) return show(status)
      this.reportStale()
    }
    const problem = missingTools() ?? validate(profile)
    if (problem) return stop({ kind: 'failed', message: sshMessage(problem) })
    show({ kind: 'connecting', step: 'Reading SSH settings…' })
    const config = await run(SSH, resolveArguments(profile))
    if (token.cancelled) return idle
    const resolved = config.status === 0 ? parseConfig(config.stdout) : null
    if (!resolved) return stop({ kind: 'failed', message: sshMessage("SSH couldn't read the settings for {name}.", { name: profile.host }, lastLine(config.stderr)) })
    const name = knownHostsName(resolved)

    // Host key (never by reading ssh's error text).
    const recorded = await this.recordedKeys(name)
    if (recorded.size === 0) {
      if (usesProxy(resolved)) {
        return stop({ kind: 'failed', message: sshMessage('{name} goes through a jump host. Connect once with ssh in Terminal to confirm its host key, then try again.', { name: profile.host }) })
      }
      show({ kind: 'connecting', step: 'Reading the host key…' })
      const lines = knownHostsLines(await scanHostKeys(resolved), name)
      if (lines.length === 0) return { kind: 'retry', message: sshMessage("Can't reach {address}.", { address: `${resolved.hostname}:${resolved.port}` }) }
      const prints = await run(KEYGEN, ['-l', '-f', '-'], lines.join('\n') + '\n')
      const fingerprints = prints.stdout.split(/\r?\n/).filter(Boolean)
      if (prints.status !== 0 || fingerprints.length === 0) return stop({ kind: 'failed', message: sshMessage("Couldn't read the host key of {name}.", { name: profile.host }) })
      this.pendingKeys.set(id, lines)
      return stop({ kind: 'confirmHostKey', name, fingerprints })
    }

    show({ kind: 'connecting', step: 'Signing in…' })
    const shell = await this.shells.of(profile, token)
    if (token.cancelled) return idle
    const result = await run(SSH, infoArguments(profile, home, identity.dataFolder, dataDir, shell))
    if (token.cancelled) return idle
    if (result.status === 255) {
      if (authRefused(result.stderr)) return stop({ kind: 'failed', message: signInRefused(profile) })
      // Windows reads only the one key the host presents, which needn't be the pinned key's type.
      if (!usesProxy(resolved) && process.platform !== 'win32') {
        const scan = await run(KEYSCAN, keyscanArguments(resolved))
        const current = hostKeys(scan.stdout)
        // Only plain host keys can be compared: a `@cert-authority` key is never what keyscan returns.
        const pinned = await this.recordedKeys(name, false)
        if (current.size > 0 && pinned.size > 0 && ![...current].some((k) => pinned.has(k))) {
          return stop({ kind: 'failed', message: sshMessage("The host key of {name} changed. This can mean someone is intercepting the connection. Codync won't connect until the old key is removed from known_hosts.", { name: profile.host }) })
        }
      }
      return { kind: 'retry', message: sshMessage("SSH couldn't connect to {name}.", { name: profile.host }, lastLine(result.stderr)) }
    }
    if (result.status === 127) return stop({ kind: 'notInstalled', shell })
    const info = parseInfo(result.stdout)
    if (!info) {
      if (result.status === 0) return stop({ kind: 'notInstalled', shell })
      // The computer's default shell may have changed since the probe: ask again on the next attempt.
      this.shells.forget(id)
      return stop({ kind: 'failed', message: sshMessage("codync-host on {name} couldn't report its identity. Run `codync-host install` there.", { name: profile.host }, lastLine(result.stderr)) })
    }
    if (!consistent(info.computerId, info.signKey)) return stop({ kind: 'failed', message: sshMessage('{name} reported an invalid identity.', { name: profile.host }) })
    if (profile.computerId && profile.computerId !== info.computerId) {
      return stop({ kind: 'failed', message: sshMessage('{name} is now a different computer than the one saved. If it was reinstalled, remove it and add it again.', { name: profile.host }) })
    }
    if (!profile.computerId) this.remember(id, info.computerId, info.name)
    if (!info.running) return { kind: 'retry', message: sshMessage('codync-host is installed on {name} but not running. Run `codync-host install` there.', { name: info.name }) }

    // Tunnel: a free loopback port each attempt; a port taken in between just makes ssh exit (ExitOnForwardFailure).
    let tunnelFailure = sshMessage("Couldn't open the tunnel to {name}.", { name: profile.host })
    for (let attempt = 0; attempt < 3; attempt++) {
      const port = await freeLocalPort()
      if (port === null) return { kind: 'retry', message: sshMessage('No free local port for the tunnel.') }
      show({ kind: 'connecting', step: 'Opening the tunnel…' })
      const tunnel = this.startTunnel(profile, port)
      if (!tunnel) return stop({ kind: 'failed', message: sshMessage("Couldn't start ssh.") })
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
        else tunnelFailure = sshMessage("Couldn't open the tunnel to {name}.", { name: profile.host }, await tail.lastLine())
        continue
      }
      if (health.computerId !== info.computerId) {
        child.kill()
        return stop({ kind: 'failed', message: sshMessage('The tunnel to {name} reached a different computer than expected.', { name: profile.host }) })
      }
      // /health is public, so the answer alone doesn't prove it came through ssh: another
      // local user could have taken the port. Only a listener ssh owns gets the token.
      const listens = child.pid !== undefined ? await sshListens(child.pid, port) : false
      // Disconnected, removed or edited meanwhile: this attempt must not register its tunnel.
      if (token.cancelled) {
        child.kill()
        return idle
      }
      if (!listens || !alive()) {
        console.error(`ssh: tunnel port ${port} answered by something other than ssh`)
        child.kill()
        tunnelFailure = sshMessage("Couldn't open the tunnel to {name}. Another program answered on its local port.", { name: profile.host })
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
      show({ kind: 'connected', localPort: port })
      return { kind: 'attached' }
    }
    if (authRefused(tunnelFailure.detail ?? '')) return stop({ kind: 'failed', message: signInRefused(profile) })
    return { kind: 'retry', message: tunnelFailure }
  }

  private async recordedKeys(name: string, certAuthorities = true) {
    const keys = new Set<string>()
    for (const file of knownHostsFiles(home, dataDir)) {
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
      child = spawn(SSH, tunnelArguments(profile, localPort, home, dataDir), { stdio: ['ignore', 'ignore', 'pipe'], windowsHide: true })
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
        this.setStatus(id, { kind: 'retrying', message: sshMessage('The SSH connection dropped.', {}, detail) })
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
export function registerSSH(getWindows: () => BrowserWindow[], reportStale: () => void) {
  const ssh = new SSHComputers(join(app.getPath('userData'), 'ssh-profiles.json'), (state) => {
    for (const w of getWindows()) if (!w.isDestroyed()) w.webContents.send('ssh:change', state)
  }, reportStale)
  ipcMain.handle('ssh:state', () => ssh.state)
  ipcMain.handle('ssh:save', (_e, profile: SSHProfile) => ssh.save(profile))
  ipcMain.on('ssh:remove', (_e, id: string) => ssh.remove(id))
  ipcMain.on('ssh:connect', (_e, id: string) => ssh.connect(id))
  ipcMain.on('ssh:disconnect', (_e, id: string) => ssh.disconnect(id))
  ipcMain.on('ssh:trustHostKey', (_e, id: string) => void ssh.trustHostKey(id))
  ipcMain.on('ssh:install', (_e, id: string) => void ssh.install(id))
  ipcMain.handle('ssh:chooseKey', async () => {
    const result = await dialog.showOpenDialog({ defaultPath: join(home, '.ssh'), properties: ['openFile', 'showHiddenFiles'] })
    return result.canceled ? null : (result.filePaths[0] ?? null)
  })
  // ssh children outlive the app unless stopped.
  app.on('will-quit', () => ssh.disconnectAll())
  void ssh.connectAll()
}
