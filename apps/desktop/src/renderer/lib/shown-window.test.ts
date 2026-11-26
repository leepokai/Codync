import assert from 'node:assert/strict'
import { test } from 'node:test'
import { entryFrom, entryIdsOf, indexOfAnyAnchor, indexOfEntry, pageAbove, pagePlan, pinnedWindowStart, revealWindowStart, rowAnchors, startShowing, windowAnchors, windowStart, type WindowRow } from './shown-window.ts'

/** A row that shows host entry `n`; a separator shows none, exactly as `buildChat` builds them. */
const entry = (n: number): WindowRow => ({ kind: 'entry', entry: { id: `e${n}` } })
const sep = (): WindowRow => ({ kind: 'separator' })
/** A grouped row: `ids` are the host entries inside it, and the first is the group's primary entry. */
const group = (...ids: string[]): WindowRow => ({
  kind: 'exchanges',
  entry: { id: ids[0]! },
  exchanges: ids.map((id) => ({ id })),
})
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

test('a pinned window follows the first row it was showing through deletions and regrouping above it', () => {
  // No separators here, so an entry's index is its own row and a deletion is plain to see.
  const rows = Array.from({ length: 80 }, (_, i) => entry(i))
  const anchors = rowAnchors(rows, 40)
  assert.equal(pinnedWindowStart(rows, 40, anchors, 40), 40)
  // The reader's window started at e40. Deleting e0 above it must not turn the same index into e41 and drop e40.
  const afterDelete = rows.filter((row) => row.entry?.id !== 'e0')
  assert.equal(pinnedWindowStart(afterDelete, 40, anchors, 40), 39)
  assert.equal(afterDelete[pinnedWindowStart(afterDelete, 40, anchors, 40)]!.entry!.id, 'e40')
  // Two rows above it settle into one group. Its index moves again; the anchor still names the same first row.
  const afterGroup = [group('e1', 'e2'), ...rows.slice(3)]
  assert.equal(pinnedWindowStart(afterGroup, 40, anchors, 40), 38)
  assert.equal(afterGroup[pinnedWindowStart(afterGroup, 40, anchors, 40)]!.entry!.id, 'e40')
  // The first anchored row itself goes away: the next row the reader was seeing becomes the first one, not e41 by
  // arithmetic and not the newest page.
  const withoutFirst = rows.filter((row) => row.entry?.id !== 'e40')
  assert.equal(withoutFirst[pinnedWindowStart(withoutFirst, 40, anchors, 40)]!.entry!.id, 'e41')
  // Every anchor gone (a compacted conversation) falls back to the clamped index rather than jumping to the newest.
  assert.equal(pinnedWindowStart([], 40, anchors, 40), 0)
})

test('the newest page carries no anchors, so a new arrival still moves its start', () => {
  const rows = Array.from({ length: 60 }, (_, i) => entry(i))
  assert.deepEqual(windowAnchors(rows, null, 40), [])
  // A pinned page does carry them, and holds its first row while rows arrive below.
  const pinned = windowAnchors(rows, 10, 40)
  assert.equal(pinned[0], 'e10')
  const afterArrival = [...rows, entry(60)]
  assert.equal(pinnedWindowStart(afterArrival, 10, pinned, 40), 10)
  assert.equal(afterArrival.slice(10).length, 51)
  // The follower's window moves one row the same arrival, staying one page.
  assert.equal(pinnedWindowStart(afterArrival, null, windowAnchors(rows, null, 40), 40), 21)
  assert.equal(afterArrival.slice(21).length, 40)
})

test('a pin whose first row is a separator keeps that separator until the list itself removes it', () => {
  const rows: WindowRow[] = [{ id: 'sep-e0', kind: 'separator' }, { ...entry(0), id: 'e0' }, { id: 'sep-e1', kind: 'separator' }, { ...entry(1), id: 'e1' }]
  const anchors = rowAnchors(rows, 0)
  assert.deepEqual(anchors.slice(0, 3), ['sep-e0', 'e0', 'sep-e1'])
  // The pin is not silently advanced past its own separator on the next render.
  assert.equal(pinnedWindowStart(rows, 0, anchors, 2), 0)
  // A row above it goes away: the separator, not the same numeric index's next row, is still the first row.
  const withOlder = [{ ...entry(9), id: 'e9' }, ...rows]
  const afterDelete = withOlder.filter((row) => row.id !== 'e9')
  assert.equal(pinnedWindowStart(withOlder, 1, rowAnchors(withOlder, 1), 2), 1)
  assert.equal(pinnedWindowStart(afterDelete, 1, rowAnchors(withOlder, 1), 2), 0)
  assert.equal(afterDelete[pinnedWindowStart(afterDelete, 1, rowAnchors(withOlder, 1), 2)]!.id, 'sep-e0')
})

test('a grouped first anchored row is found by an entry that is no longer its primary', () => {
  const before = [entry(0), entry(1), entry(2), entry(3)]
  const anchors = rowAnchors(before, 2) // e2, e3 (these test rows carry no row id of their own)
  const grouped = [entry(0), group('e1', 'e2', 'e3')]
  assert.equal(indexOfAnyAnchor(grouped, anchors), 1)
  assert.equal(pinnedWindowStart(grouped, 2, anchors, 1), 1)
  assert.deepEqual(entryIdsOf(grouped[1]!), ['e1', 'e2', 'e3'])
})

test('a press that exposes only a cached separator also asks the computer for the older page', () => {
  const rows = [sep(), entry(1), sep(), entry(2)]
  // The reader's first entry is e1, at row 1. A page is wider than everything above it, so the first press lands at
  // the separator at row 0. With older rows on the computer, that same press must fetch rather than stop at it.
  assert.deepEqual(pagePlan(rows, 'e1', 40, true), { start: 0, fetch: true })
  // No older rows to ask for: opening the cached separator is the whole press.
  assert.deepEqual(pagePlan(rows, 'e1', 40, false), { start: 0, fetch: false })
  // Plenty of rows held above the reader: the press opens them locally.
  const deep = Array.from({ length: 100 }, (_, i) => entry(i))
  assert.deepEqual(pagePlan(deep, 'e80', 40, true), { start: 40, fetch: false })
  assert.deepEqual(pagePlan(deep, 'missing', 40, true), { start: null, fetch: false })
})

test('a trace-only catch-up asks the host once instead of making older chat rows unreachable', () => {
  // `buildChat` has filtered every entry out, so there is no row for `wasShowing`; the computer still has more.
  assert.deepEqual(pagePlan([], null, 40, true), { start: null, fetch: true })
  // These pressed rows carry no other state: once the computer answers with a visible row, the next plan can show it.
  assert.deepEqual(pagePlan([entry(0)], null, 40, true), { start: null, fetch: false })
  // With nothing above on the host either, one press is the whole request: the caller does not loop on `fetch`.
  assert.deepEqual(pagePlan([], null, 40, false), { start: null, fetch: false })
})

test('a page fully held above the reader opens locally, while a lone separator still asks the host', () => {
  // The newest page began at e40 and every e0..e39 row is already cached. This is a cached page, not the list's start.
  const cached = Array.from({ length: 80 }, (_, i) => entry(i))
  assert.deepEqual(pagePlan(cached, 'e40', 40, true), { start: 0, fetch: false })
  // The boundary is visible rows, not the index: a separator alone above the reader is not a page of messages.
  const separatorOnly = [sep(), entry(1), sep(), entry(2)]
  assert.deepEqual(pagePlan(separatorOnly, 'e1', 40, true), { start: 0, fetch: true })
  // A partially cached page also opens what is held; older messages are fetched by the next press at the top.
  assert.deepEqual(pagePlan([entry(0), entry(1), entry(2), entry(3)], 'e2', 40, true), { start: 0, fetch: false })
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

test('a grouped row keeps a place filed under any entry inside it, including one a fetch merges above', () => {
  // The reader's first row was the exchange x1 on its own. Older exchanges with the same peer arrive, and x1 is
  // merged into x0's group: its id is no longer the row's primary entry, but the row still shows it.
  const before = [{ kind: 'exchanges', entry: { id: 'x1' }, exchanges: [{ id: 'x1' }] }]
  assert.equal(indexOfEntry(before, 'x1'), 0)
  const merged = [group('x0', 'x1'), entry(2)]
  assert.equal(indexOfEntry(merged, 'x1'), 0)
  assert.equal(entryFrom(merged, 0), 'x0')
  // The renderer puts every one of these on the row, so the scroll hold can find it after the merge.
  assert.deepEqual(entryIdsOf(merged[0]!), ['x0', 'x1'])
  assert.deepEqual(entryIdsOf(sep()), [])
  // Approval rows carry their entries too, so the same holds when older approvals join a row.
  const approvals: WindowRow[] = [{ kind: 'requests', entry: { id: 'a0' }, entries: [{ id: 'a0' }, { id: 'a1' }] }]
  assert.equal(indexOfEntry(approvals, 'a1'), 0)
  // An entry no row shows is still not a place.
  assert.equal(indexOfEntry(merged, 'x9'), null)
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

test('revealing the newest card keeps a follower a follower, without stale anchors', () => {
  const rows = Array.from({ length: 80 }, (_, i) => entry(i))
  // The newest card is already in the newest 40. A numeric 40 here would leave anchors on the follower, and the next
  // arrival would grow the transcript to 41 rows.
  assert.equal(revealWindowStart(rows, 'e79', null, 40, true), null)
  // A numeric window that happens to reach the newest page also becomes the plain follower.
  assert.equal(revealWindowStart(rows, 'e45', 40, 40, true), null)
  // A follower that first opened a fully cached page has start 0 and all 80 rows shown. Revealing the newest card
  // must still return to the plain follower page, not keep the expanded window and grow to 81 on the next arrival.
  assert.equal(revealWindowStart(rows, 'e79', 0, 40, true), null)
  // Revealing an older card opens the history at it, even from a numeric window.
  assert.equal(revealWindowStart(rows, 'e2', 40, 40, true), 2)
  // A reader already in history keeps the explicit window a reveal asks for.
  assert.equal(revealWindowStart(rows, 'e79', 20, 40, false), 20)
  assert.equal(revealWindowStart(rows, 'e2', 20, 40, false), 2)
  // A card not in the list does not turn a stale numeric start into follower anchors; neither does it move history.
  assert.equal(revealWindowStart(rows, 'missing', 20, 40, true), null)
  assert.equal(revealWindowStart(rows, 'missing', 20, 40, false), 20)
})
