// Writes host/src/screen/cua_tools.json: the driver tools bots get, as MCP tools. The host offers
// exactly these names, reads which ones only look from them, and lists them when the driver can't
// start. Run after changing driver.json or TOOLS: node packaging/cua-driver/tools.mjs <cua-driver>
import { spawn } from 'node:child_process'
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs'
import { connect } from 'node:net'
import { tmpdir } from 'node:os'
import { dirname, join } from 'node:path'
import { fileURLToPath } from 'node:url'

// Background-safe GUI work. Left out: bring_to_front and kill_app (take over or end the user's
// apps), clipboard_read (whatever the user copied), browser_* (each browser asks for Automation),
// and the driver's own configuration, recording, session and update tools.
const TOOLS = [
  'list_apps', 'list_windows', 'get_window_state', 'get_desktop_state', 'zoom', 'launch_app',
  'click', 'double_click', 'right_click', 'drag', 'scroll', 'type_text', 'press_key', 'hotkey',
  'set_value', 'invoke_menu',
]

const [driver] = process.argv.slice(2)
if (!driver) throw new Error('usage: tools.mjs <path to cua-driver>')
const dir = mkdtempSync(join(tmpdir(), 'cua-'))
const socket = join(dir, 'c.sock')
const daemon = spawn(driver, ['serve', '--socket', socket, '--no-overlay'], {
  env: { ...process.env, CUA_DRIVER_EMBEDDED: '1', CUA_DRIVER_RS_TELEMETRY_ENABLED: '0', CUA_DRIVER_RS_UPDATE_CHECK: '0' },
  stdio: 'ignore',
})
try {
  const list = await request(socket, { method: 'list', client_kind: 'cli' })
  const byName = new Map(list.tools.map((t) => [t.name, t]))
  const tools = TOOLS.map((name) => {
    const t = byName.get(name)
    if (!t) throw new Error(`the driver has no ${name} tool`)
    return {
      name,
      description: t.description,
      inputSchema: t.input_schema,
      annotations: { readOnlyHint: t.read_only, destructiveHint: t.destructive, idempotentHint: t.idempotent, openWorldHint: t.open_world },
    }
  })
  const out = join(dirname(fileURLToPath(import.meta.url)), '../../host/src/screen/cua_tools.json')
  writeFileSync(out, `${JSON.stringify(tools, null, 1)}\n`)
  console.log(`${tools.length} tools → ${out}`)
} finally {
  daemon.kill()
  rmSync(dir, { recursive: true, force: true })
}

/** One request on the daemon's line protocol, retried until it listens. */
async function request(path, body) {
  for (let i = 0; ; i++) {
    try {
      return await new Promise((resolve, reject) => {
        const s = connect(path)
        let buf = ''
        s.on('connect', () => s.write(`${JSON.stringify(body)}\n`))
        s.on('data', (d) => {
          buf += d
          const end = buf.indexOf('\n')
          if (end < 0) return
          s.destroy()
          const reply = JSON.parse(buf.slice(0, end))
          reply.ok ? resolve(reply.result) : reject(new Error(reply.error))
        })
        s.on('error', reject)
      })
    } catch (e) {
      if (i > 50) throw e
      await new Promise((r) => setTimeout(r, 200))
    }
  }
}
