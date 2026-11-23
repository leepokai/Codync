import assert from 'node:assert/strict'
import { test } from 'node:test'
import { noVisits, step, visit } from './visits.ts'

test('Back and Forward walk the places visited, and a new place drops what lay ahead', () => {
  const same = (a: string, b: string) => a === b
  let v = noVisits<string>()
  for (const place of ['a', 'b', 'b', 'c']) v = visit(v, place, same)
  assert.deepEqual(v, { places: ['a', 'b', 'c'], at: 2 }, 'the same place twice is one visit')
  assert.equal(step(v, 1), null)
  const back = step(v, -1)!
  assert.equal(back.places[back.at], 'b')
  assert.equal(visit(back, 'b', same), back, 'landing there is not a new visit')
  assert.deepEqual(step(back, 1), v)
  assert.deepEqual(visit(back, 'd', same), { places: ['a', 'b', 'd'], at: 2 })
  assert.equal(step(step(back, -1)!, -1), null)
  let long = noVisits<string>()
  for (let i = 0; i < 60; i++) long = visit(long, String(i), same)
  assert.deepEqual([long.places.length, long.places[0], long.at], [50, '10', 49])
})
