import assert from 'node:assert/strict'
import { test } from 'node:test'
import { dictationEngineIds, voiceEngine, voiceEngineIds } from './voice-engine.ts'

test('Windows and Linux never offer or restore the unsupported on-device engine', () => {
  for (const platform of ['win32', 'linux']) {
    assert.deepEqual(voiceEngineIds(platform), ['openai', 'gemini'])
    assert.equal(voiceEngine('device', platform), 'openai')
    assert.equal(voiceEngine('gemini', platform), 'gemini')
  }
})

test('macOS keeps on-device dictation and provider choices', () => {
  assert.deepEqual(voiceEngineIds('darwin'), ['device', 'openai', 'gemini'])
  assert.equal(voiceEngine('device', 'darwin'), 'device')
})

test("the computer's own speech model is a dictation engine everywhere, never a call engine", () => {
  assert.deepEqual(dictationEngineIds('darwin'), ['device', 'openai', 'gemini', 'local'])
  assert.deepEqual(dictationEngineIds('linux'), ['openai', 'gemini', 'local'])
  for (const platform of ['darwin', 'linux']) assert.ok(!voiceEngineIds(platform).includes('local'), platform)
})
