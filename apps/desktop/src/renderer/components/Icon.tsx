import { useSyncExternalStore, type CSSProperties } from 'react'
import { fallbackSvg } from './icon-fallbacks'

export type Weight = 'light' | 'regular' | 'medium' | 'semibold' | 'bold' | 'black'

interface Metrics {
  w: number
  h: number
  baseline: number
}

// SF Symbols masks exported at build time on macOS (tools/export-symbols.swift); elsewhere
// (Linux, Windows) the Lucide fallbacks draw instead.
let manifest: Record<string, Metrics> | null = null
const listeners = new Set<() => void>()
if (window.codync?.platform === 'darwin') {
  void fetch(`${import.meta.env.BASE_URL}symbols/manifest.json`)
    .then((r) => (r.ok ? r.json() : {}))
    .then((m: Record<string, Metrics>) => {
      manifest = m
      for (const l of listeners) l()
    })
    .catch(() => {
      manifest = {}
      for (const l of listeners) l()
    })
} else {
  manifest = {}
}

function useManifest() {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l)
      return () => void listeners.delete(l)
    },
    () => manifest,
  )
}

interface Props {
  /** SF Symbol name. */
  name: string
  /** Point size of the font the symbol is drawn in. */
  size: number
  weight?: Weight
  color?: string
  style?: CSSProperties
  className?: string
  /** Scales with the app's text size (`.appFont`); off for fixed-size chrome. */
  scaled?: boolean
  title?: string
}

/** An SF Symbol (`Image(systemName:)`) in the current text color. */
export function Icon({ name, size, weight = 'regular', color, style, className, scaled = true, title }: Props) {
  const m = useManifest()
  const metrics = m?.[name]
  const unit = scaled ? `${size}px * var(--scale)` : `${size}px`
  if (metrics) {
    const url = `url("${import.meta.env.BASE_URL}symbols/${name}.${weight}.png")`
    return (
      <span
        className={className}
        title={title}
        aria-hidden={title ? undefined : true}
        style={{
          display: 'inline-block',
          flex: 'none',
          width: `calc(${metrics.w} * ${unit})`,
          height: `calc(${metrics.h} * ${unit})`,
          backgroundColor: color ?? 'currentColor',
          WebkitMaskImage: url,
          maskImage: url,
          WebkitMaskSize: '100% 100%',
          maskSize: '100% 100%',
          WebkitMaskRepeat: 'no-repeat',
          ...style,
        }}
      />
    )
  }
  // Lucide fallback, about the same visual size as the symbol.
  const stroke = { light: 1.5, regular: 1.75, medium: 2, semibold: 2.25, bold: 2.5, black: 2.75 }[weight]
  return (
    <svg
      className={className}
      aria-hidden={title ? undefined : true}
      viewBox="0 0 24 24"
      fill="none"
      stroke={color ?? 'currentColor'}
      strokeWidth={stroke}
      strokeLinecap="round"
      strokeLinejoin="round"
      style={{ width: `calc(1.2 * ${unit})`, height: `calc(1.2 * ${unit})`, flex: 'none', ...style }}
      dangerouslySetInnerHTML={{ __html: (title ? `<title>${title}</title>` : '') + fallbackSvg(name) }}
    />
  )
}
