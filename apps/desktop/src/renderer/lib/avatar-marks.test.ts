import assert from 'node:assert/strict'
import { test } from 'node:test'
import { crownSize, pauseMarkSize, pauseMarkTop } from './avatar-marks.ts'

/** The roster icon sizes the Appearance slider can reach: 24 to 44, step 2. */
const SIZES = Array.from({ length: 11 }, (_, i) => 24 + i * 2)

test('the crown follows the icon size, so a big icon gets a big crown', () => {
  // Ryan: "I like the crown maybe dump it up in size also" — and it must scale with the slider
  // (before, it was floored at 12px and barely grew).
  assert.equal(crownSize(24), 12)
  assert.equal(crownSize(30), 12.6)
  assert.equal(crownSize(44), 18.48)
  // Never shrinks as the icon grows …
  for (let i = 1; i < SIZES.length; i++)
    assert.ok(crownSize(SIZES[i]!) >= crownSize(SIZES[i - 1]!), `crown never shrinks from ${SIZES[i - 1]} to ${SIZES[i]}`)
  // … and above the 12px floor it really follows the slider, step by step.
  for (let i = 1; i < SIZES.length; i++) {
    if (crownSize(SIZES[i - 1]!) <= 12) continue
    assert.ok(crownSize(SIZES[i]!) > crownSize(SIZES[i - 1]!), `crown grows from ${SIZES[i - 1]} to ${SIZES[i]}`)
  }
})

test('the crown is never smaller than it used to be, at any icon size', () => {
  // The old floor was Math.max(12, size * 0.34). The new one is never less.
  for (const size of SIZES) {
    const before = Math.max(12, size * 0.34)
    assert.ok(crownSize(size) >= before, `crown at ${size} is at least the old ${before}`)
  }
})

test('the pause mark is sized by the icon alone, never by the crown', () => {
  // The defect: a lead bot that was also paused lost its mark at the smallest icon, because the
  // mark was shrunk into the gap the crown left. Size no longer depends on the crown at all.
  for (const size of SIZES) {
    assert.equal(pauseMarkSize(size), Math.max(9, size * 0.3))
    assert.ok(pauseMarkSize(size) >= 9, `the mark stays readable at ${size}`)
    assert.ok(pauseMarkSize(size) >= 8, `the mark is never skipped at ${size}`)
  }
})

test('the pause mark scales with the icon too, so the smallest icon still reads', () => {
  assert.equal(pauseMarkSize(24), 9)
  assert.equal(pauseMarkSize(44), 13.2)
  for (let i = 1; i < SIZES.length; i++)
    assert.ok(pauseMarkSize(SIZES[i]!) >= pauseMarkSize(SIZES[i - 1]!), `mark grows from ${SIZES[i - 1]} to ${SIZES[i]}`)
})

test('a lead bot that is paused keeps its mark, at every icon size', () => {
  // Ryan could not test the both-lead-and-paused case. It is the one that used to fail.
  for (const size of SIZES) {
    const mark = pauseMarkSize(size)
    assert.ok(mark >= 8, `a lead+paused bot at ${size} still wears a mark (${mark})`)
    // And it is nudged clear of the crown rather than shrunk to fit.
    assert.ok(pauseMarkTop(size, true) > 0, `the mark at ${size} moves clear of the crown`)
  }
})

test('a bot that is not lead needs no nudging', () => {
  for (const size of SIZES) assert.equal(pauseMarkTop(size, false), 0)
})

test('the nudge clears the part of the crown that hangs below the frame edge', () => {
  // The crown's box is translate(-50%, -44%) with a square box, so about 56% of its width hangs
  // below the frame's top edge. The mark's top has to start at or past that, or the crown covers it.
  for (const size of SIZES) {
    const overhang = crownSize(size) * 0.56
    assert.ok(pauseMarkTop(size, true) >= overhang, `mark at ${size} starts below the crown's overhang`)
  }
})

test('the nudge plus the mark never pushes the mark out of the frame', () => {
  // The mark is pinned top-right; it must still sit inside the icon at the smallest size.
  for (const size of SIZES) {
    const bottom = pauseMarkTop(size, true) + pauseMarkSize(size)
    assert.ok(bottom <= size, `crown ${crownSize(size)} + mark ${pauseMarkSize(size)} fits in ${size}`)
  }
})
