// Artifacts and the Library on the wire (`host/src/chat/artifacts.rs`, `host/src/chat/library.rs`).
import type { Attachment, ReplyQuote, SharedFile } from './models'
import { isBelow } from './compat.ts'
import { decodeFileChunk, sha256Hex } from './files.ts'

/** An HTML artifact a bot published; `readArtifact` returns its HTML. */
export interface Artifact {
  id: string
  botId: string
  title: string
  size: number
  sha256: string
  createdAt: number
  updatedAt: number
  /** Its newest card in the bot's chat: what a message about the artifact replies to. */
  entryId?: string
}

/** The desktop app loads each artifact from `codync-artifact://<id>/` (`main/artifacts.ts`), an origin of its own. */
export const ARTIFACT_SCHEME = 'codync-artifact'

/** An artifact frame's address; a card's preview is marked, so it opens nothing. */
export const artifactFrameURL = (id: string, version: number, preview: boolean) =>
  `${ARTIFACT_SCHEME}://${id}/?v=${version}${preview ? '&preview' : ''}`

/** An artifact frame's address, a card's or the viewer's. */
export const isArtifactURL = (url: string | undefined): url is string => !!url?.startsWith(`${ARTIFACT_SCHEME}:`)

/** An artifact's viewer frame: its scheme, without a card's `preview` marker. */
export const isArtifactViewer = (url: string | undefined) => isArtifactURL(url) && !new URL(url).searchParams.has('preview')

/** How long after a click or a key press a link it follows may open; later, it's a script acting alone. */
const FOLLOWED_MS = 1000

/** Whether what an artifact opens follows the user: a click or key in the window, after its viewer loaded, just now. */
export const followsUser = (input: number, loaded: number, now: number) => input > loaded && now - input < FOLLOWED_MS

/** One thing in a chat's Library (`library`): an artifact, a file the bot shared, or one the user attached. */
export type LibraryItem =
  | { kind: 'artifact'; at: number; artifact: Artifact }
  | { kind: 'file'; at: number; entryId: string; file: SharedFile }
  | { kind: 'attachment'; at: number; entryId: string; file: Attachment }

/** An artifact card in the chat (`data.artifact`): the artifact as it was published then. */
export interface ArtifactCard {
  id: string
  title: string
  updatedAt: number
}

/** One `readArtifact` answer. */
export interface ArtifactChunk { data: string; size: number; artifact: Artifact }

/** The first host with artifacts and the Library. */
export const ARTIFACTS_HOST = '2.25.0'

/** Whether the computer's host has the Library; an older one has no `library` method, so the app leaves it out. */
export function hasLibrary(hostVersion: string | undefined): boolean {
  return !!hostVersion && !isBelow(hostVersion, ARTIFACTS_HOST)
}

/** How a message about the artifact quotes its card (`chat::replies`) until the computer's own quote arrives; none
 * without a card (a routine published it). */
export function artifactQuote(artifact: Artifact): ReplyQuote | null {
  return artifact.entryId ? { id: artifact.entryId, author: artifact.botId, text: `Artifact: ${artifact.title}`, artifact: artifact.id } : null
}

/** The draft of the message box under an artifact, apart from its chat's (`ComposerDrafts.removeBot` drops both). */
export function artifactDraftKey(artifact: Artifact): string {
  return `${artifact.botId}/artifact/${artifact.id}`
}

/**
 * An artifact's whole HTML, read chunk by chunk with `read(offset)`: every chunk of one version (published again
 * meanwhile, the new version is read from its start), checked against its SHA-256.
 */
export async function readArtifactHTML(read: (offset: number) => Promise<ArtifactChunk>): Promise<{ artifact: Artifact; bytes: Uint8Array }> {
  let html = new Uint8Array(0)
  let offset = 0
  let version: string | null = null
  for (;;) {
    const { artifact, ...chunk } = await read(offset)
    if (artifact.sha256 !== version) {
      const restart = offset > 0
      version = artifact.sha256
      html = new Uint8Array(artifact.size)
      offset = 0
      if (restart) continue
    }
    const bytes = decodeFileChunk(chunk, artifact.size, offset)
    html.set(bytes, offset)
    offset += bytes.length
    if (offset < artifact.size) continue
    if ((await sha256Hex(html)) !== version) throw new Error('The artifact failed its integrity check; try again')
    return { artifact, bytes: html }
  }
}
