import assert from 'node:assert/strict'
import { test } from 'node:test'
import { fixtureCommand } from './fixture-command.ts'

test('Windows runs the fixture through cmd.exe with python and double quotes', () => {
  const command = fixtureCommand('C:\\repo\\host\\tests\\fixtures\\artifact_agent.py', 'win32')
  assert.equal(command, 'python -X utf8 -u "C:\\repo\\host\\tests\\fixtures\\artifact_agent.py"')
})

test('macOS and Linux keep python3 and single quotes', () => {
  assert.equal(fixtureCommand('/repo/host/tests/fixtures/artifact_agent.py', 'darwin'), "python3 -X utf8 -u '/repo/host/tests/fixtures/artifact_agent.py'")
  assert.equal(fixtureCommand('/repo/host/tests/fixtures/artifact_agent.py', 'linux'), "python3 -X utf8 -u '/repo/host/tests/fixtures/artifact_agent.py'")
})
