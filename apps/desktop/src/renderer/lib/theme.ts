import { useSyncExternalStore } from 'react'

/**
 * A media query read from one MediaQueryList and listened to on another: Chromium shares a list's
 * `matches` with its change detection, so a read while the window is hidden can swallow the `change`
 * and memoized canvases (avatars) keep the old appearance's ink. Showing or focusing the window
 * checks again, so a missed change never outlives the next look at the window.
 */
function mediaQuery(media: string) {
  const reads = window.matchMedia(media)
  const events = window.matchMedia(media)
  return {
    get: () => reads.matches,
    subscribe: (cb: () => void) => {
      events.addEventListener('change', cb)
      document.addEventListener('visibilitychange', cb)
      window.addEventListener('focus', cb)
      return () => {
        events.removeEventListener('change', cb)
        document.removeEventListener('visibilitychange', cb)
        window.removeEventListener('focus', cb)
      }
    },
  }
}

const darkQuery = mediaQuery('(prefers-color-scheme: dark)')
const motionQuery = mediaQuery('(prefers-reduced-motion: reduce)')

/**
 * The screen's pixel ratio, for canvases' backing size. It changes when the window moves to another
 * screen or the Mac sleeps with the lid closed; a canvas drawn meanwhile would stay blurry at 1x.
 * The query names the current ratio, so it is renewed on every change.
 */
const pixelRatio = {
  get: () => window.devicePixelRatio || 1,
  subscribe: (cb: () => void) => {
    let query: MediaQueryList | null = null
    const changed = () => {
      watch()
      cb()
    }
    const watch = () => {
      query?.removeEventListener('change', changed)
      query = window.matchMedia(`(resolution: ${pixelRatio.get()}dppx)`)
      query.addEventListener('change', changed)
    }
    watch()
    document.addEventListener('visibilitychange', changed)
    window.addEventListener('focus', changed)
    return () => {
      query?.removeEventListener('change', changed)
      document.removeEventListener('visibilitychange', changed)
      window.removeEventListener('focus', changed)
    }
  },
}

export function useDark() {
  return useSyncExternalStore(darkQuery.subscribe, darkQuery.get)
}

export function useReduceMotion() {
  return useSyncExternalStore(motionQuery.subscribe, motionQuery.get)
}

export function usePixelRatio() {
  return useSyncExternalStore(pixelRatio.subscribe, pixelRatio.get)
}

export const isDark = darkQuery.get
export const reduceMotion = motionQuery.get

/** Palette values for drawing code (canvas) that can't read CSS variables; they match theme.css. */
export const palette = (dark: boolean) => ({
  text: dark ? [250, 250, 250] : [20, 20, 20],
  warning: '#F0A030',
  danger: dark ? '#F0A7A7' : '#C23A2B',
  added: dark ? '#8fd18b' : '#2e7d32',
  tertiary: dark ? '#9a9a9a' : '#6e6e6e',
})

export interface Swatch {
  id: string
  label: string
  hex: string
}

export const AVATAR_COLORS: Swatch[] = [
  { id: 'black', label: 'Black', hex: '#2B2B2B' },
  { id: 'brown', label: 'Brown', hex: '#936439' },
  { id: 'red', label: 'Red', hex: '#FF263C' },
  { id: 'orange', label: 'Orange', hex: '#FF6700' },
  { id: 'yellow', label: 'Yellow', hex: '#FF9800' },
  { id: 'green', label: 'Green', hex: '#00C972' },
  { id: 'cyan', label: 'Cyan', hex: '#00BCA6' },
  { id: 'blue', label: 'Blue', hex: '#1084FE' },
  { id: 'violet', label: 'Violet', hex: '#9159FE' },
  { id: 'magenta', label: 'Magenta', hex: '#FF309B' },
  { id: 'gray', label: 'Gray', hex: '#777777' },
]

export const AVATAR_SHAPES = ['blob', 'pebble', 'squircle', 'tablet', 'wedge', 'hex', 'cloud', 'teardrop']

export function avatarColor(id: string) {
  return (AVATAR_COLORS.find((c) => c.id === id) ?? AVATAR_COLORS[7]!).hex
}

/** `[r, g, b]` → `#RRGGBB`. */
export const rgbHex = (rgb: number[]) => `#${rgb.map((c) => c.toString(16).padStart(2, '0')).join('')}`

/** `#RRGGBB` → `rgba(r, g, b, a)`. */
export function withAlpha(hex: string, alpha: number) {
  const n = parseInt(hex.slice(1), 16)
  return `rgba(${(n >> 16) & 255}, ${(n >> 8) & 255}, ${n & 255}, ${alpha})`
}
