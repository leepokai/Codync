import assert from 'node:assert/strict'
import { test } from 'node:test'
import { keyDetail, keyRows, keyVars, newKeyRow } from './login-profile-keys.ts'

test('the key form opens on the saved variables, else the agent\'s usual names, else one empty row', () => {
  assert.deepEqual(keyRows({ vars: ['E2E_KEY'] }, ['USUAL_KEY']), [{ id: 0, name: 'E2E_KEY', value: '', saved: true, typed: false }])
  assert.deepEqual(keyRows({ vars: [] }, ['USUAL_KEY', 'USUAL_URL']).map((r) => [r.name, r.saved, r.typed]), [['USUAL_KEY', false, false], ['USUAL_URL', false, false]])
  assert.deepEqual(keyRows({}, []), [newKeyRow(0)])
  assert.equal(newKeyRow(3).typed, true)
})

test('save sends typed values under their names and removed variables as empty', () => {
  const rows = [
    { id: 0, name: 'KEPT', value: '', saved: true, typed: false },
    { id: 1, name: ' NEW_NAME ', value: 'v1', saved: false, typed: true },
    { id: 2, name: '', value: 'no name', saved: false, typed: true },
  ]
  assert.deepEqual(keyVars(rows, ['GONE']), { GONE: '', NEW_NAME: 'v1' })
  assert.deepEqual(keyVars([{ id: 0, name: 'GONE', value: 'again', saved: false, typed: true }], ['GONE']), { GONE: 'again' }, 'typed again after removing')
  assert.deepEqual(keyVars(rows.slice(0, 1), []), {}, 'nothing typed: nothing to save')
})

test('an API key row lists its variable names', () => {
  assert.deepEqual(keyDetail({ vars: ['A', 'B'] }), ['API key', 'A, B'])
  assert.deepEqual(keyDetail({ vars: [] }), ['API key', 'No key yet'])
  assert.deepEqual(keyDetail({}), ['API key', 'No key yet'])
})
