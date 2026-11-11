import assert from 'node:assert/strict'
import { test } from 'node:test'
import { installLine, installing } from './tools.ts'

const echo = (text: string) => text

test('the download line says how far it got, then that it unpacks, and nothing otherwise', () => {
  const downloading = { phase: 'downloading' as const, done: 1024 * 1024 * 1024, total: 2 * 1024 * 1024 * 1024, error: null }
  assert.equal(installLine('ComfyUI', downloading, echo), 'Downloading ComfyUI… 1.0 GB of 2.0 GB')
  assert.equal(installLine('ComfyUI', { phase: 'unpacking', done: 0, total: 0, error: null }, echo), 'Unpacking ComfyUI…')
  assert.equal(installLine('ComfyUI', { phase: 'idle', done: 0, total: 0, error: null }, echo), null)
  assert.equal(installLine('ComfyUI', undefined, echo), null, 'an older host has no download')
})

test('only a download or an unpack counts as arriving', () => {
  assert.equal(installing({ phase: 'downloading', done: 0, total: 1, error: null }), true)
  assert.equal(installing({ phase: 'unpacking', done: 0, total: 0, error: null }), true)
  assert.equal(installing({ phase: 'failed', done: 0, total: 0, error: 'HTTP 503' }), false)
  assert.equal(installing(undefined), false)
})

test('the line is translated as a whole, with its numbers filled in', () => {
  const spanish = (text: string) => (text === 'Downloading {name}… {done} of {total}' ? 'Descargando {name}… {done} de {total}' : text)
  const line = installLine('ComfyUI', { phase: 'downloading', done: 500 * 1024 * 1024, total: 2 * 1024 * 1024 * 1024, error: null }, spanish)
  assert.equal(line, 'Descargando ComfyUI… 500 MB de 2.0 GB')
})
