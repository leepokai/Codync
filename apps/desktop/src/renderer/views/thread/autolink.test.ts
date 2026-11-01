import assert from 'node:assert/strict'
import { test } from 'node:test'
import { splitURLs } from './autolink.ts'

const urls = (text: string) => splitURLs(text).flatMap((p) => (typeof p === 'string' ? [] : [p.href]))

// The same cases as the iPhone's, Android's and the terminal's autolink tests.
test('finds bare addresses and leaves the sentence around them', () => {
  assert.deepEqual(urls('see https://codync.dev.'), ['https://codync.dev'])
  assert.deepEqual(urls('open www.example.com/a?b=1, then'), ['https://www.example.com/a?b=1'])
  assert.deepEqual(urls('(at https://example.com/x)'), ['https://example.com/x'])
  assert.deepEqual(urls('https://en.wikipedia.org/wiki/Rust_(language)'), ['https://en.wikipedia.org/wiki/Rust_(language)'])
  assert.deepEqual(urls('網址：https://codync.dev，謝謝'), ['https://codync.dev'])
  assert.deepEqual(urls('HTTP://A.COM and http://b.io/x_y_z'), ['http://A.COM', 'http://b.io/x_y_z'])
  assert.deepEqual(urls('https://a.com/**bold**'), ['https://a.com/'])
})

test('skips what only looks like an address', () => {
  assert.deepEqual(urls('main.rs README.md codync.dev'), [])
  assert.deepEqual(urls('xhttps://a.com nohttp'), [])
  assert.deepEqual(urls('https:// and www.'), [])
})

test('keeps the text around the address', () => {
  assert.deepEqual(splitURLs('a https://b.c d'), ['a ', { url: 'https://b.c', href: 'https://b.c' }, ' d'])
})

test('a long run of closing brackets is trimmed in one pass', () => {
  const started = Date.now()
  assert.deepEqual(urls(`https://a.com/${')'.repeat(200_000)}`), ['https://a.com/'])
  assert.ok(Date.now() - started < 1000)
})
