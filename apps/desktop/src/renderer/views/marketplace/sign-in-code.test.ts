import assert from 'node:assert/strict'
import { test } from 'node:test'
import { SignInCode } from './sign-in-code.ts'

test('the dialog closing before its action still opens the page', async () => {
  const prompt = new SignInCode()
  const answer = prompt.ask('ABCD-1234')
  assert.equal(prompt.code, 'ABCD-1234')
  // Dialog calls onClose, then the action.
  prompt.close()
  prompt.open()
  assert.equal(await answer, true)
  assert.equal(prompt.code, null)
})

test('closing alone cancels, and a new code cancels the old one', async () => {
  const prompt = new SignInCode()
  const first = prompt.ask('AAAA-1111')
  const second = prompt.ask('BBBB-2222')
  assert.equal(await first, false)
  prompt.close()
  assert.equal(await second, false)
  assert.equal(prompt.code, null)
})
