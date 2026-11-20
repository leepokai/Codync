import type { PickedFile } from '@shared/ipc'

/** The files' contents; one that can't be read (a copied folder) is left out instead of failing the rest. */
export async function readPicked(files: File[]): Promise<PickedFile[]> {
  const read = await Promise.all(
    files.map((f) => f.arrayBuffer().then((b) => ({ name: f.name || `pasted-${Date.now()}.png`, data: new Uint8Array(b) }), () => null)),
  )
  return read.filter((f) => f !== null)
}

/**
 * ⌘V with files: the computer's own clipboard (real names and contents) when it has them, else the files the
 * paste event held. `held` must be taken while the event runs: its clipboard data is empty once it ends.
 */
export async function pastedFiles(fromComputer: () => Promise<PickedFile[]>, held: File[]): Promise<PickedFile[]> {
  const picked = await fromComputer().catch(() => [])
  return picked.length ? picked : readPicked(held)
}
