import { spawn } from 'node:child_process'
import { mkdtempSync } from 'node:fs'
import { tmpdir } from 'node:os'
import { join } from 'node:path'
const p = spawn(process.execPath, ['bridge.mjs', mkdtempSync(join(tmpdir(), 'wa-'))], { stdio: ['pipe','pipe','pipe'] })
p.stdout.on('data', d => process.stdout.write(d))
p.stderr.on('data', d => process.stderr.write(d))
p.stdin.write(JSON.stringify({type:'start'}) + '\n')
setTimeout(() => { p.kill(); process.exit(0) }, 20000)
