import assert from 'node:assert/strict'
import { test } from 'node:test'
import { goneValue, notificationFailure, errorReport, parseStack } from './error-report.ts'

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

test('a process that ended says why, which helper, and its exit code', () => {
  assert.equal(goneValue({ reason: 'crashed', exitCode: -1073741819, name: 'Network Service' }, 'win32'), 'crashed · Network Service · exit 0xC0000005')
  assert.equal(goneValue({ reason: 'oom', exitCode: 9 }, 'darwin'), 'oom · exit 9')
})

test('a failed toast keeps only the step and its HRESULT', () => {
  assert.equal(notificationFailure('WinAPI: Show failed, ERROR Notification failed - Settings prevent the notification from being delivered. (HRESULT: -2143420143)'), 'WinAPI: Show failed 0x803E0111')
  assert.equal(notificationFailure('XML: Invalid XML, ERROR  (HRESULT: -1072897500)'), 'XML: Invalid XML 0xC00CE224')
  assert.equal(notificationFailure('Notification failed - Toast Notification was dropped without being displayed to the user. (HRESULT: -2143419897)'), 'Toast failed 0x803E0207')
  assert.equal(notificationFailure('something about C:\\\\Users\\\\kevin'), 'Other')
})
