import assert from 'node:assert/strict'
import { test } from 'node:test'
import { errorReport, parseStack } from './error-report.ts'

test('a report keeps the error class and its frames, never the message', () => {
  const error = new TypeError('cannot read /Users/kevin/secret.txt')
  error.stack = [
    'TypeError: cannot read /Users/kevin/secret.txt',
    '    at loadChat (file:///Applications/Codync.app/Contents/Resources/app.asar/out/renderer/assets/index-a1.js:99:2)',
    '    at async open (file:///Applications/Codync.app/Contents/Resources/app.asar/out/renderer/assets/index-a1.js:10:4)',
    '    at file:///Applications/Codync.app/Contents/Resources/app.asar/out/renderer/assets/index-a1.js:5:1',
  ].join('\n')
  const report = errorReport(error, 'desktop-renderer', 'onerror')
  assert.equal(report.type, 'TypeError')
  assert.ok(!JSON.stringify(report).includes('secret'))
  assert.deepEqual(report.frames.map((f) => [f.function, f.line]), [['<anonymous>', 5], ['async open', 10], ['loadChat', 99]])
})

test('a thrown non-error has a type but nothing else', () => {
  assert.deepEqual(errorReport('hello world', 'desktop-main', 'uncaught'), { process: 'desktop-main', type: 'NonError', mechanism: 'uncaught', frames: [] })
  assert.deepEqual(parseStack('Error: x\n    at node:internal/process/task_queues:105:5'), [{ function: '<anonymous>', file: 'node:internal/process/task_queues', line: 105, column: 5 }])
})
