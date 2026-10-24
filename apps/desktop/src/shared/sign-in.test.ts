import assert from 'node:assert/strict'
import { test } from 'node:test'
import { clerkStrategy, signInProviderName } from './sign-in.ts'

test('WeChat uses Clerk\'s custom OIDC strategy and the built-in providers keep theirs', () => {
  assert.equal(clerkStrategy('apple'), 'oauth_apple')
  assert.equal(clerkStrategy('google'), 'oauth_google')
  assert.equal(clerkStrategy('wechat'), 'oauth_custom_wechat')
  assert.deepEqual(signInProviderName, { apple: 'Apple', google: 'Google', wechat: 'WeChat' })
})
