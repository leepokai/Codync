import assert from 'node:assert/strict'
import { readFileSync, writeFileSync } from 'node:fs'
import { test } from 'node:test'
import { ORB_REST, orbCells, orbFrame, type OrbState } from './thinking-orb.ts'

const STATES: OrbState[] = ['working', 'searching', 'listening', 'connecting', 'shaping']
// Shared with the iPhone and Android tests, so the three ports draw the same dots.
const FIXTURE = new URL('../../../../ios/Kit/Tests/CodyncKitTests/Fixtures/thinking-orb-frames.json', import.meta.url)
const round = (v: number) => Math.round(v * 1e5) / 1e5

function cases() {
  return STATES.flatMap((state) => [13, 20, 32].flatMap((size) => [0.37, 1.3, 2.9].map((t) => ({
    state, size, t, dots: orbFrame(state, size, t).map((d) => [d.x, d.y, d.radius, d.alpha].map(round)),
  }))))
}

test('frames and still poses match the shared fixture (CODYNC_WRITE_ORB_FRAMES=1 rewrites it)', () => {
  if (process.env.CODYNC_WRITE_ORB_FRAMES) {
    const lines = cases().map((c) => `    ${JSON.stringify(c)}`)
    writeFileSync(FIXTURE, `{\n  "tolerance": 0.0001,\n  "rest": ${JSON.stringify(ORB_REST)},\n  "cases": [\n${lines.join(',\n')}\n  ]\n}\n`)
  }
  const fixture = JSON.parse(readFileSync(FIXTURE, 'utf8')) as { tolerance: number; rest: typeof ORB_REST; cases: ReturnType<typeof cases> }
  assert.deepEqual(fixture.rest, ORB_REST)
  for (const expected of fixture.cases) {
    const dots = orbFrame(expected.state, expected.size, expected.t)
    assert.equal(dots.length, expected.dots.length, `${expected.state} ${expected.size} ${expected.t}`)
    dots.forEach((d, i) => [d.x, d.y, d.radius, d.alpha].forEach((v, k) =>
      assert.ok(Math.abs(v - expected.dots[i]![k]!) <= fixture.tolerance, `${expected.state} ${expected.size} ${expected.t} dot ${i}`)))
  }
})

test('the grid is cropped to a circle: 5 × 5 under 18 pt, 7 × 7 under 30 pt, then 9 × 9', () => {
  assert.deepEqual([13, 16, 18, 20, 30, 64].map(orbCells), [5, 5, 7, 7, 9, 9])
  assert.deepEqual([13, 20, 32].map((size) => orbFrame('working', size, 0).length), [21, 37, 69])
})

test('dots never move, only their size and ink change', () => {
  for (const state of STATES) {
    const at = (t: number) => orbFrame(state, 16, t)
    assert.deepEqual(at(0).map((d) => [d.x, d.y]), at(1.7).map((d) => [d.x, d.y]))
    assert.notDeepEqual(at(0).map((d) => d.radius), at(1.5).map((d) => d.radius), state)
    for (const d of at(0.9)) assert.ok(d.radius >= 16 / 5 * 0.14 - 1e-9 && d.radius <= 16 / 5 * 0.5 + 1e-9 && d.alpha >= 0.5 && d.alpha <= 1)
  }
})

test('each state rests on a pose of its own', () => {
  const poses = STATES.map((s) => JSON.stringify(orbFrame(s, 16, ORB_REST[s]).map((d) => round(d.radius))))
  assert.equal(new Set(poses).size, STATES.length)
})

test('bad input draws nothing instead of throwing', () => {
  assert.deepEqual(orbFrame('working', 0, 1), [])
  assert.deepEqual(orbFrame('shaping', 16, Number.NaN), [])
  assert.equal(orbFrame('shaping', 16, -3).length, 21)
})
