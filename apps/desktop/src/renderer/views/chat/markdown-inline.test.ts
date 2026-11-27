import assert from 'node:assert/strict'
import { test } from 'node:test'
import { inlineParts, type InlinePart } from './markdown-inline.ts'

const text = (parts: InlinePart[]): string =>
  parts.map((p) => {
    if (p.kind === 'text' || p.kind === 'code') return p.text
    return text(p.children)
  }).join('')

test('inline: bold and italic wrap an inline code span without a bare marker', () => {
  for (const source of ['**文字 `code` 文字**', '**`code`**', '***code***', '___code___', '**a `x*y` b**']) {
    const parts = inlineParts(source)
    assert.ok(parts.length > 0, source)
    assert.ok(!text(parts).includes('**'), `${source}: ${text(parts)}`)
    assert.ok(parts.some((p) => p.kind === 'strong'), `${source}: bold`)
    if (source.includes('`')) {
      const codes: string[] = []
      const collect = (list: InlinePart[]) => {
        for (const p of list) {
          if (p.kind === 'code') codes.push(p.text)
          else if (p.kind !== 'text') collect(p.children)
        }
      }
      collect(parts)
      assert.equal(codes.length, 1)
      assert.equal(codes[0]!.includes('*') || codes[0] === 'code', true)
    }
  }
})

test('inline: code still beats emphasis inside a span', () => {
  assert.deepEqual(inlineParts('`**not bold**`'), [{ kind: 'code', text: '**not bold**' }])
  assert.deepEqual(inlineParts('**`a *b* c`**'), [
    { kind: 'strong', children: [{ kind: 'code', text: 'a *b* c' }] },
  ])
})

test('inline: links, strike and italic still parse', () => {
  assert.deepEqual(inlineParts('[link](https://x) ~~gone~~ *here*'), [
    { kind: 'link', url: 'https://x', children: [{ kind: 'text', text: 'link' }] },
    { kind: 'text', text: ' ' },
    { kind: 'strike', children: [{ kind: 'text', text: 'gone' }] },
    { kind: 'text', text: ' ' },
    { kind: 'em', children: [{ kind: 'text', text: 'here' }] },
  ])
})

test('inline: a delimiter inside a code span does not close the run around it', () => {
  assert.deepEqual(inlineParts('**a `x**y` b**'), [
    {
      kind: 'strong',
      children: [
        { kind: 'text', text: 'a ' },
        { kind: 'code', text: 'x**y' },
        { kind: 'text', text: ' b' },
      ],
    },
  ])
  assert.deepEqual(inlineParts('**`**`**'), [
    { kind: 'strong', children: [{ kind: 'code', text: '**' }] },
  ])
  // Underscores inside code stay code too, and a lone backtick stays text.
  assert.deepEqual(inlineParts('_a `b_c` d_'), [
    {
      kind: 'em',
      children: [
        { kind: 'text', text: 'a ' },
        { kind: 'code', text: 'b_c' },
        { kind: 'text', text: ' d' },
      ],
    },
  ])
})
