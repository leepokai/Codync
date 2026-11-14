import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { TrayUsageComputer } from './ipc.ts'
import { usageRows } from './tray-usage.ts'

const line = (title: string) => ({ title, bar: null })
const codex = { name: 'Codex', icon: null, logins: [{ name: '', lines: [line('Weekly · 44% used')] }] }
const claude = {
  name: 'Claude',
  icon: null,
  logins: [
    { name: 'me@example.com', lines: [line('Session · 13% used')] },
    { name: 'Work', lines: [line('No reading yet')] },
  ],
}
const computer = (name: string | null, agents = [claude, codex]): TrayUsageComputer => ({ name, symbol: 'laptopcomputer', agents })

test('one computer lists its agents in the menu, and an agent with several logins opens them', () => {
  assert.deepEqual(usageRows([computer(null)], 'Usage limits'), [
    {
      label: 'Claude',
      image: null,
      rows: [
        { title: 'me@example.com' },
        { label: 'Session · 13% used', image: null },
        { rule: true },
        { title: 'Work' },
        { label: 'No reading yet', image: null },
      ],
    },
    { label: 'Codex', image: null },
    { label: 'Weekly · 44% used', image: null },
  ])
})

test('several computers each open their own agents, under a title', () => {
  const rows = usageRows([computer('Laptop'), computer('Mini', [codex])], 'Usage limits')
  assert.deepEqual(rows.map((row) => ('title' in row ? row.title : 'label' in row ? row.label : null)), ['Usage limits', 'Laptop', 'Mini'])
  assert.deepEqual(rows[2], { label: 'Mini', symbol: 'laptopcomputer', rows: [{ label: 'Codex', image: null }, { label: 'Weekly · 44% used', image: null }] })
  assert.deepEqual(usageRows([], 'Usage limits'), [], 'nothing to show, no title')
})
