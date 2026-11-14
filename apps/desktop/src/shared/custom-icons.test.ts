import assert from 'node:assert/strict'
import { test } from 'node:test'
import { validIcon } from './custom-icons.ts'

test('custom icons accept only bounded PNG data, never remote or active content', () => {
  assert.ok(validIcon({ dataUrl: 'data:image/png;base64,iVBORw0KGgo=', tinted: false }))
  for (const dataUrl of ['https://example.com/icon.png', 'data:image/svg+xml,<svg/>', 'data:image/png;base64,!', `data:image/png;base64,${'A'.repeat(65559)}`]) {
    assert.equal(validIcon({ dataUrl, tinted: false }), false)
  }
  assert.equal(validIcon(undefined), false)
})
