import assert from 'node:assert/strict'
import { test } from 'node:test'
import { agentRows, bytesText, healthAlert, healthLines, levelLabel, thresholdProblem, type HostHealth } from './health.ts'

const GB = 1024 ** 3
const health = (over: Partial<HostHealth> = {}): HostHealth => ({
  ramTotal: 8 * GB, ramFree: 2 * GB, ramUsedPercent: 75, cpu: 12.4, diskTotal: 500 * GB, diskFree: 120 * GB,
  level: 'warning', refusing: false, thresholds: { warn: 70, critical: 85 }, agents: [], at: 0, ...over,
})

test('bytes are told in the unit a person reads', () => {
  assert.equal(bytesText(1.5 * GB), '1.5 GB')
  assert.equal(bytesText(320 * 1024 * 1024), '320 MB')
  assert.equal(bytesText(0), '0 MB')
})

test('the small alert says nothing when all is well and says why turns stop when they do', () => {
  assert.equal(healthAlert(null), null)
  assert.equal(healthAlert(health({ level: 'normal', ramUsedPercent: 40 })), null)
  assert.equal(healthAlert(health()), 'Memory 75 % · getting full')
  assert.equal(healthAlert(health({ level: 'critical', refusing: true, ramUsedPercent: 90 })), 'Memory 90 % · not starting new turns')
  assert.equal(levelLabel('critical'), 'Memory is full')
  assert.equal(levelLabel('normal'), 'Healthy')
})

test('the numbers come in the order memory, CPU, disk', () => {
  assert.deepEqual(healthLines(health()), [
    { label: 'Memory', value: '75 % used · 2.0 GB free of 8.0 GB' },
    { label: 'CPU', value: '12 %' },
    { label: 'Disk', value: '120.0 GB free of 500.0 GB' },
  ])
  assert.equal(healthLines(health({ diskTotal: 0 }))[2]?.value, 'Unknown')
})

test('agents are listed biggest first', () => {
  const rows = agentRows(health({ agents: [
    { botId: 'a', botName: 'Small', pid: 1, rss: 200 * 1024 * 1024 },
    { botId: 'b', botName: 'Big', pid: 2, rss: 2 * GB },
  ] }))
  assert.deepEqual(rows.map((r) => [r.name, r.use]), [['Big', '2.0 GB'], ['Small', '200 MB']])
})

test('a threshold pair is checked like the host checks it', () => {
  assert.equal(thresholdProblem(70, 85), null)
  assert.match(thresholdProblem(90, 80) ?? '', /before the limit/)
  assert.match(thresholdProblem(0, 80) ?? '', /between 1 and 99/)
  assert.match(thresholdProblem(50, 100) ?? '', /between 1 and 99/)
  assert.match(thresholdProblem(50.5, 80) ?? '', /whole numbers/)
})
