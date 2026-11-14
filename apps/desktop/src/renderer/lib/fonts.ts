import type { CSSProperties } from 'react'

// macOS text styles (NSFont.preferredFont) mapped onto the type ramp in styles/theme.css, which
// scales with the app's text size preference like kit's `AppFont`.
const TextStyle = {
  largeTitle: 'large-title',
  title: 'title',
  title2: 'title2',
  title3: 'title3',
  headline: 'body',
  body: 'body',
  callout: 'callout',
  subheadline: 'caption',
  // macOS draws these at 10; 11 is the floor for text people read or click.
  footnote: 'caption',
  caption: 'caption',
  caption2: 'caption',
  /** Codync's compact desktop body and secondary text. */
  compactBody: 'callout',
  compactSecondary: 'caption',
} as const

export type TextStyleName = keyof typeof TextStyle

/** `calc(<pt>px * var(--scale))` */
const px = (size: number) => `calc(${size}px * var(--scale))`

export type Weight = 'regular' | 'medium' | 'semibold' | 'bold' | 'black' | 'light'
export const weights: Record<Weight, number> = { light: 300, regular: 400, medium: 500, semibold: 600, bold: 700, black: 900 }

/** `.appFont(...)`: a text style name or a point size, with weight, optionally monospaced. */
export function font(style: TextStyleName | number, weight?: Weight, mono = false): CSSProperties {
  const w = weight ?? (style === 'headline' ? 'bold' : 'regular')
  return {
    fontSize: typeof style === 'number' ? px(style) : `var(--text-${TextStyle[style]})`,
    fontWeight: weights[w],
    ...(mono ? { fontFamily: 'var(--mono)' } : {}),
  }
}
