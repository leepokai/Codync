import assert from 'node:assert/strict'
import { test } from 'node:test'
import { entryFrom, indexOfEntry, pageAbove, startShowing, windowStart, type WindowRow } from './shown-window.ts'

/** A row that shows host entry `n`; a separator shows none, exactly as `buildChat` builds them. */
const entry = (n: number): WindowRow => ({ kind: 'entry', entry: { id: `e${n}` } })
const sep = (): WindowRow => ({ kind: 'separator' })
/** A conversation of `count` entries, each with a date separator above it — so entry `n` sits at row `2n + 1`. */
const chatOf = (count: number, first = 0) => Array.from({ length: count }, (_, i) => [sep(), entry(first + i)]).flat()

test('a window with no place of its own is the newest page', () => {
  assert.equal(windowStart(300, null, 40), 260)
  assert.equal(windowStart(10, null, 40), 0)
})

test('a window is a number, so it survives the list being rebuilt around it', () => {
  // The reader had paged up to the oldest rows held: the window started at row 0 of 161 rows.
  assert.equal(windowStart(161, 0, 40), 0)
  // 100 older rows arrived above theirs: the window starts 100 further down, whatever the ids now look like.
  assert.equal(windowStart(261, 100, 40), 100)
  // Rows went away: the window keeps as much of what is left as it can, rather than jumping to the newest page.
  assert.equal(windowStart(80, 200, 40), 79)
  assert.equal(windowStart(0, 200, 40), 0)
})

test('an entry is found where it sits however the list is grouped', () => {
  const rows = chatOf(3) // sep, e0, sep, e1, sep, e2
  // Entry 1 sits below its own separator, at row 3.
  assert.equal(indexOfEntry(rows, 'e1'), 3)
  assert.equal(indexOfEntry(rows, 'e9'), null)
  assert.equal(indexOfEntry(rows, null), null)
  // A separator shows no entry, so asking for its id must not be answered by the entry below it.
  assert.equal(indexOfEntry(rows, 'sep-e1'), null)
})

test('the reader\'s place is the entry their first row is about, not the separator above it', () => {
  // The reported case: the window's first row is a separator, which exists only because the list began there.
  const rows = chatOf(3)
  assert.equal(entryFrom(rows, 0), 'e0')
  // A list whose first row is an entry: that entry.
  assert.equal(entryFrom([entry(5), sep(), entry(6)], 0), 'e5')
  // Skips over however many separators and merged rows come first.
  assert.equal(entryFrom([sep(), sep(), entry(0)], 0), 'e0')
  assert.equal(entryFrom([], 0), null)
})

test('the page asked for is a page above the reader\'s line, and the top of the list when nothing is above it', () => {
  // The reader's first entry is now row 101 of the rebuilt list: a page of 40 above it starts at 61.
  assert.equal(pageAbove(101, 40), 61)
  // Only 10 rows above it: those are the page, and the view starts at their top.
  assert.equal(pageAbove(10, 40), 0)
  // Nothing above it at all: the window starts at the top of the list, which is where the reader already is.
  assert.equal(pageAbove(0, 40), 0)
})

test('a task\'s card further down brings the window to it; one already shown leaves the window alone', () => {
  const rows = chatOf(50) // 100 rows: sep above every entry, so entry n sits at row 2n + 1
  // Entry 2 is at row 5; the window still shows the newest 40 rows of 100, which starts at 60: it moves down to it.
  assert.equal(startShowing(rows, 'e2', null, 40), 5)
  // Entry 45 is above the newest-40 start, so the window is already showing it: no move.
  assert.equal(startShowing(rows, 'e45', null, 40), 60)
  // An entry that is not in the list at all leaves the window exactly where it was.
  assert.equal(startShowing(rows, 'e99', 12, 40), 12)
  assert.equal(startShowing(rows, null, 12, 40), 12)
})
