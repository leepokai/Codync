import { useLayoutEffect, useRef, type ClipboardEvent, type KeyboardEvent, type RefObject } from 'react'
import { composerEnter } from './composer-keyboard'

/**
 * A message box's text field: it grows with its text up to eight lines. Return sends, Shift-Return adds a line, and
 * an input method's Return picks its candidate. `onKey` takes a key first (a menu's arrows, Escape) and says whether
 * it did.
 */
export function ComposerField({ field, value, placeholder, label, onChange, onSend, onKey, onPaste }: {
  field: RefObject<HTMLTextAreaElement | null>
  value: string
  placeholder: string
  label: string
  onChange: (text: string) => void
  onSend: () => void
  onKey?: (e: KeyboardEvent<HTMLTextAreaElement>, inInputMethod: boolean) => boolean
  onPaste?: (e: ClipboardEvent<HTMLTextAreaElement>) => void
}) {
  const measure = useRef<HTMLDivElement>(null)
  const composing = useRef(false)

  // Measure a separate, wrapping copy: collapsing the live textarea to zero breaks its
  // height transition, caret scroll and IME composition. Observe font/width changes too.
  useLayoutEffect(() => {
    const el = field.current
    const mirror = measure.current
    if (!el || !mirror) return
    const resize = () => {
      const style = getComputedStyle(el)
      const line = parseFloat(style.lineHeight)
      const padding = parseFloat(style.paddingTop) + parseFloat(style.paddingBottom)
      const height = Math.min(mirror.getBoundingClientRect().height, line * 8 + padding)
      el.style.height = `${height}px`
    }
    resize()
    const observer = new ResizeObserver(resize)
    observer.observe(mirror)
    return () => observer.disconnect()
  }, [field])

  return (
    <div className="composer-field">
      <div ref={measure} className="composer-measure" aria-hidden="true">{value + '\u200b'}</div>
      <textarea
        ref={field}
        rows={1}
        value={value}
        placeholder={placeholder}
        spellCheck
        aria-label={label}
        aria-keyshortcuts="Enter Shift+Enter"
        onCompositionStart={() => { composing.current = true }}
        onCompositionEnd={() => { composing.current = false }}
        onChange={(e) => onChange(e.target.value)}
        onKeyDown={(e) => {
          const isComposing = composing.current || e.nativeEvent.isComposing
          if (onKey?.(e, isComposing || e.keyCode === 229)) return
          const action = composerEnter({ key: e.key, shiftKey: e.shiftKey, altKey: e.altKey, repeat: e.repeat, isComposing, keyCode: e.keyCode })
          if (e.key === 'Enter' && e.repeat && !e.shiftKey && !e.altKey) e.preventDefault()
          if (action === 'send') {
            e.preventDefault()
            onSend()
          }
        }}
        onPaste={onPaste}
      />
    </div>
  )
}
