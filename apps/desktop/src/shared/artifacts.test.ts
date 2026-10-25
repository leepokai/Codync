import assert from 'node:assert/strict'
import { createHash } from 'node:crypto'
import { test } from 'node:test'
import { artifactFrameURL, artifactQuote, followsUser, hasLibrary, isArtifactURL, isArtifactViewer, readArtifactHTML, type Artifact } from './artifacts.ts'
import { FILE_CHUNK_SIZE } from './files.ts'

function version(text: string, updatedAt: number) {
  const bytes = Buffer.from(text)
  const sha256 = createHash('sha256').update(bytes).digest('hex')
  const artifact: Artifact = { id: 'a', botId: 'b', title: 'Report', size: bytes.length, sha256, createdAt: 0, updatedAt }
  return { bytes, artifact }
}

/** A host whose `readArtifact` answers from each version in turn, staying on the last one. */
function host(...versions: ReturnType<typeof version>[]) {
  const offsets: number[] = []
  const read = async (offset: number) => {
    const { bytes, artifact } = versions[Math.min(offsets.length, versions.length - 1)]!
    offsets.push(offset)
    return { data: bytes.subarray(offset, offset + FILE_CHUNK_SIZE).toString('base64'), size: artifact.size, artifact }
  }
  return { read, offsets }
}

test('an artifact republished while it is read is read again from its start', async () => {
  const first = version('a'.repeat(FILE_CHUNK_SIZE + 10), 1)
  const second = version('b'.repeat(FILE_CHUNK_SIZE + 20), 2)
  const { read, offsets } = host(first, second)
  const html = await readArtifactHTML(read)
  assert.equal(html.artifact.updatedAt, 2)
  assert.deepEqual(Buffer.from(html.bytes), second.bytes)
  assert.deepEqual(offsets, [0, FILE_CHUNK_SIZE, 0, FILE_CHUNK_SIZE], 'never one version spliced onto another')
})

test('HTML that fails its digest is refused', async () => {
  const stored = version('<h1>Hi</h1>', 1)
  const { read } = host({ ...stored, bytes: Buffer.from('<h1>Ho</h1>') })
  await assert.rejects(readArtifactHTML(read), /integrity/)
})

test('the Library needs a host that has it, and only a card takes messages about its artifact', () => {
  assert.equal(hasLibrary('2.25.0'), true)
  assert.equal(hasLibrary('2.24.3'), false)
  assert.equal(hasLibrary(undefined), false)
  const { artifact } = version('x', 1)
  assert.equal(artifactQuote(artifact), null, 'a routine published it')
  assert.deepEqual(artifactQuote({ ...artifact, entryId: 'e' }), { id: 'e', author: 'b', text: 'Artifact: Report', artifact: 'a' })
})

test('an artifact opens only what the user just followed in its viewer', () => {
  const viewer = artifactFrameURL('a1', 5, false)
  const card = artifactFrameURL('a1', 5, true)
  assert.equal(isArtifactURL(card), true)
  assert.equal(isArtifactURL('https://example.com/'), false)
  assert.equal(isArtifactViewer(viewer), true)
  assert.equal(isArtifactViewer(card), false, 'a card opens nothing')
  assert.equal(isArtifactViewer('https://example.com/'), false)
  assert.equal(isArtifactViewer(undefined), false)
  assert.equal(followsUser(1_500, 1_000, 1_600), true, 'a click after the viewer loaded')
  assert.equal(followsUser(900, 1_000, 1_100), false, 'a redirect on load right after the click that opened the viewer')
  assert.equal(followsUser(1_500, 1_000, 3_000), false, 'a script acting long after the last click')
  assert.equal(followsUser(0, 0, 10), false, 'no input yet')
})
