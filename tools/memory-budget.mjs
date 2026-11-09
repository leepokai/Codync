// Prints this computer's health and what each running agent uses, and fails when the level is critical.
// Usage: node tools/memory-budget.mjs [port] [--json]
import { readFile } from 'node:fs/promises'
import { homedir } from 'node:os'
import { join } from 'node:path'

const port = Number(process.argv[2] ?? 19223)
const json = process.argv.includes('--json')
const tokenFile = process.env.CODYNC_TOKEN_FILE ?? join(homedir(), '.codync-dev', 'token')
const token = (await readFile(tokenFile, 'utf8')).trim()

const answer = await fetch(`http://127.0.0.1:${port}/api/hostHealth`, {
  method: 'POST',
  headers: { Authorization: `Bearer ${token}`, 'Content-Type': 'application/json' },
  body: '{}',
})
if (!answer.ok) {
  console.error(`hostHealth: HTTP ${answer.status}`)
  process.exit(2)
}
const health = await answer.json()
if (json) {
  console.log(JSON.stringify(health, null, 2))
} else {
  console.log(`Level: ${health.level} (warn ${health.thresholds.warn} %, stop ${health.thresholds.critical} %)`)
  console.log(`Memory: ${health.ramUsedPercent} % used, ${Math.round((health.ramFree ?? 0) / 1024 / 1024)} MB free`)
  for (const row of health.agents ?? []) {
    console.log(`  ${row.botName ?? row.name}: ${Math.round((row.rss ?? 0) / 1024 / 1024)} MB`)
  }
}
process.exit(health.level === 'critical' ? 1 : 0)
