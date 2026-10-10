import { randomUUID } from 'node:crypto'
import { BrowserWindow, dialog, ipcMain } from 'electron'
import type { SharedFile } from '../shared/models'
import { saveName, validateSharedFile } from '../shared/files'
import { FileSink } from './file-sink'

export function registerFileSaves() {
  const saves = new Map<string, { owner: number; sink: FileSink; dispose: () => void }>()
  ipcMain.handle('files:begin', async (event, meta: SharedFile) => {
    validateSharedFile(meta)
    const window = BrowserWindow.fromWebContents(event.sender)
    if (!window) throw new Error('Window closed')
    const result = await dialog.showSaveDialog(window, { defaultPath: saveName(meta.name), buttonLabel: 'Save' })
    if (result.canceled || !result.filePath || event.sender.isDestroyed()) return null
    const id = randomUUID()
    const sink = await FileSink.create(result.filePath, meta, id)
    if (event.sender.isDestroyed()) { await sink.cancel(); return null }
    const destroyed = () => { saves.delete(id); void sink.cancel().catch(console.error) }
    saves.set(id, { owner: event.sender.id, sink, dispose: () => event.sender.removeListener('destroyed', destroyed) })
    event.sender.once('destroyed', destroyed)
    return id
  })
  function owned(owner: number, id: string) {
    const save = saves.get(id)
    if (!save || save.owner !== owner) throw new Error('Unknown download')
    return save.sink
  }
  function forget(id: string) { saves.get(id)?.dispose(); saves.delete(id) }
  ipcMain.handle('files:write', async (event, id: string, offset: number, bytes: Uint8Array) => {
    const sink = owned(event.sender.id, id)
    try { await sink.write(offset, bytes) } catch (error) {
      forget(id)
      await sink.cancel()
      throw error
    }
  })
  ipcMain.handle('files:finish', async (event, id: string) => {
    const sink = owned(event.sender.id, id)
    try { await sink.finish() } finally { forget(id); await sink.cancel() }
  })
  ipcMain.handle('files:cancel', async (event, id: string) => {
    const save = saves.get(id)
    if (!save) return
    const sink = owned(event.sender.id, id)
    forget(id)
    await sink.cancel()
  })
}
