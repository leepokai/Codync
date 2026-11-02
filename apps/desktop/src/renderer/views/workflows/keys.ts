// Shortcuts while the canvas is focused and the caret is not in a field.

export type Tool = 'select' | 'pan' | 'pen' | 'marker' | 'eraser' | 'note' | 'shape' | 'text'

export type KeyAction =
  | { type: 'tool'; tool: Tool }
  | { type: 'undo' }
  | { type: 'redo' }
  | { type: 'select-all' }
  | { type: 'delete' }
  | { type: 'copy' }
  | { type: 'cut' }
  | { type: 'paste' }
  | { type: 'duplicate' }
  | { type: 'zoom'; direction: 1 | -1 | 0 }
  | { type: 'fit' }
  | { type: 'run' }
  | { type: 'search' }
  | { type: 'chrome' }
  | { type: 'nudge'; dx: number; dy: number }

const TOOLS: Record<string, Tool> = { v: 'select', h: 'pan', p: 'pen', m: 'marker', e: 'eraser', n: 'note', r: 'shape' }

export function keyAction(event: { key: string; metaKey: boolean; ctrlKey: boolean; shiftKey: boolean; altKey: boolean }): KeyAction | null {
  if (event.altKey) return null
  const command = event.metaKey || event.ctrlKey
  const key = event.key
  if (command && event.shiftKey && (key === 'z' || key === 'Z')) return { type: 'redo' }
  if (command && (key === 'z' || key === 'Z')) return { type: 'undo' }
  if (command && (key === '=' || key === '+')) return { type: 'zoom', direction: 1 }
  if (command && key === '-') return { type: 'zoom', direction: -1 }
  if (command && key === '0') return { type: 'zoom', direction: 0 }
  if (command && (key === 'f' || key === 'F')) return { type: 'search' }
  if (command && key === '/') return { type: 'chrome' }
  if (command && (key === 'a' || key === 'A')) return { type: 'select-all' }
  if (command && key === 'Enter') return { type: 'run' }
  if (command && (key === 'c' || key === 'C')) return { type: 'copy' }
  if (command && (key === 'x' || key === 'X')) return { type: 'cut' }
  if (command && (key === 'v' || key === 'V')) return { type: 'paste' }
  if (command && (key === 'd' || key === 'D')) return { type: 'duplicate' }
  if (command) return null
  if (event.shiftKey && key === '1') return { type: 'fit' }
  if (key === 'ArrowLeft' || key === 'ArrowRight' || key === 'ArrowUp' || key === 'ArrowDown') {
    const step = event.shiftKey ? 32 : 8
    const dx = key === 'ArrowLeft' ? -step : key === 'ArrowRight' ? step : 0
    const dy = key === 'ArrowUp' ? -step : key === 'ArrowDown' ? step : 0
    return { type: 'nudge', dx, dy }
  }
  if (key === 'Delete' || key === 'Backspace') return { type: 'delete' }
  const tool = TOOLS[key.toLowerCase()]
  if (tool && !event.shiftKey) return { type: 'tool', tool }
  if (key.toLowerCase() === 't' && !event.shiftKey) return { type: 'tool', tool: 'text' }
  return null
}
