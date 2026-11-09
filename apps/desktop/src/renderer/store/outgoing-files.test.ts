import assert from 'node:assert/strict'
import { test } from 'node:test'
import type { Entry } from '@shared/models'
import { filesToResend, type OutgoingFile } from './outgoing-files.ts'

const file: OutgoingFile = { id: 'f1', name: 'a.txt', data: new Uint8Array([1]) }
const message = (attachments?: Entry['data']['attachments']) =>
  ({ data: { text: 'see this', clientNonce: 'n1', attachments } }) as Entry

test('Resend takes the files still kept for its message', () => {
  assert.deepEqual(filesToResend(message([{ id: 'f1', name: 'a.txt', size: 1 }]), new Map([['n1', [file]]])), [file])
})

test('Resend refuses a message whose files went with the app, so its text never goes alone', () => {
  assert.equal(filesToResend(message([{ id: 'f1', name: 'a.txt', size: 1 }]), new Map()), null)
})

test('a message without files resends with none', () => {
  assert.deepEqual(filesToResend(message(), new Map()), [])
})
