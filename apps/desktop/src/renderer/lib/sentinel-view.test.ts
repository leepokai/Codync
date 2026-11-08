import assert from 'node:assert/strict'
import { readFileSync } from 'node:fs'
import { test } from 'node:test'
import { createContext, Script } from 'node:vm'
import ts from 'typescript'
import type { SentinelReport } from '../../shared/models.ts'

interface Node {
  type: unknown
  props: Record<string, unknown>
  key?: string
}

function render(report: SentinelReport): Node[] {
  const nodes: Node[] = []
  const jsx = (type: unknown, props: Record<string, unknown>, key?: string) => {
    const node = { type, props, key }
    nodes.push(node)
    return node
  }
  let state = 0
  const local = { connection: { kind: 'online' }, bots: new Map([['bot-a', { id: 'bot-a', name: 'Reviewer' }]]) }
  const dependencies: Record<string, unknown> = {
    react: { useState: (initial: unknown) => [state++ === 0 ? report : initial, () => {}], useEffect: () => {}, useMemo: (factory: () => unknown) => factory() },
    'react/jsx-runtime': { jsx, jsxs: jsx },
    '../../components/Controls': { CardForm: 'CardForm', CardSection: 'CardSection', ValueRow: 'ValueRow' },
    '../../components/Icon': { Icon: 'Icon' },
    '../../components/Stack': { HStack: 'HStack', VStack: 'VStack' },
    '../../components/Text': { Text: 'Text' },
    '../../lib/observable': { useModel: (value: unknown) => value },
    '../../store/context': { useApp: () => ({ local }) },
    '../../store/maintenance-store': { MaintenanceStore: class {} },
    './BotBundleSection': {}, './BotCheckSection': {}, './HealthSection': {}, './TeamRunSection': {}, './TokenBudgetSection': {}, './AuditLogSection': {}, './parts': {},
  }
  const source = readFileSync(new URL('../views/settings/DiagnosticsView.tsx', import.meta.url), 'utf8')
  const output = ts.transpileModule(source, {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2023, jsx: ts.JsxEmit.ReactJSX },
    fileName: 'DiagnosticsView.tsx',
  }).outputText
  const exports: { DiagnosticsView?: () => unknown } = {}
  new Script(output).runInContext(createContext({ exports, require: (name: string) => {
    if (!(name in dependencies)) throw new Error(`Unmocked dependency: ${name}`)
    return dependencies[name]
  } }))
  assert.ok(exports.DiagnosticsView)
  exports.DiagnosticsView()
  return nodes
}

const report: SentinelReport = { scannedAt: 1_793_000_000_000, lookbackMs: 604_800_000, entriesExamined: 42, issues: [] }
const text = (nodes: Node[]) => JSON.stringify(nodes.map((node) => node.props))

test('the actual empty Sentinel report renders a successful scan with its timestamp and entry count', () => {
  const nodes = render(report)
  const content = text(nodes)
  assert.ok(content.includes('No issues found'))
  assert.ok(content.includes('Last scanned'))
  assert.ok(content.includes('42'))
  assert.ok(!content.includes('Invalid Date'))
  assert.ok(!content.includes('SQLite integrity'))
  assert.ok(!content.includes('autoFixable'))
  assert.ok(nodes.some((node) => node.type === 'Icon' && node.props.name === 'checkmark.shield.fill'))
})

test('an actual Sentinel finding renders its title, severity, identity and evidence without invented fields', () => {
  const issue = {
    signature: 'retry-signature', kind: 'retry_loop', severity: 'high', title: 'Repeated tool failure',
    toolName: 'build', botId: 'bot-a', firstSeenAt: report.scannedAt - 1000, lastSeenAt: report.scannedAt,
    count: 3, wastedTurns: 2, evidenceEntryId: 'entry-123',
  } as const
  const nodes = render({ ...report, issues: [issue] })
  const content = text(nodes)
  for (const expected of ['Repeated tool failure', 'Retry loop', 'high', 'Reviewer', 'build', 'entry-123', '1 issue']) {
    assert.ok(content.includes(expected), expected)
  }
  assert.ok(!content.includes('No issues found'))
  assert.ok(nodes.some((node) => node.type === 'Icon' && node.props.name === 'xmark.octagon.fill'))
  assert.ok(nodes.some((node) => node.key === 'retry-signature:bot-a'))
})

test('an invalid scan timestamp never renders Invalid Date', () => {
  const content = text(render({ ...report, scannedAt: Number.NaN }))
  assert.ok(content.includes('Scan time unavailable'))
  assert.ok(!content.includes('Invalid Date'))
})
