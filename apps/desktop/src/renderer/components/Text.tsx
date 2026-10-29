import type { ComponentPropsWithoutRef, CSSProperties, ReactNode } from 'react'
import { font, weights, type TextStyleName, type Weight } from '../lib/fonts'

/** The color tokens text takes (styles/theme.css). */
export type Tone = 'text' | 'secondary' | 'tertiary' | 'danger' | 'attention' | 'accent' | 'added' | 'removed'

/**
 * A run of text in a type style and tone (kit's `.appFont` + `.foregroundStyle`). Without `size`
 * it keeps the surrounding size; `truncate` ends one line with an ellipsis, `lines` clamps to n,
 * `grow` takes a stack's free space like `HStack grow`, `tabular` lines digits up.
 */
export function Text({ as = 'span', size, weight, mono = false, tone, truncate = false, lines, grow = false, tabular = false, className, style, children, ...rest }: {
  as?: 'span' | 'div' | 'h2' | 'h3'
  size?: TextStyleName | number
  weight?: Weight
  mono?: boolean
  tone?: Tone
  truncate?: boolean
  lines?: number
  grow?: boolean
  tabular?: boolean
  className?: string
  style?: CSSProperties
  children?: ReactNode
} & Omit<ComponentPropsWithoutRef<'span'>, 'className' | 'style' | 'children'>) {
  const type = size !== undefined ? font(size, weight, mono) : { fontWeight: weight && weights[weight], fontFamily: mono ? 'var(--mono)' : undefined }
  // The element only changes the tag; the props are a span's either way.
  const Tag = as as 'span'
  const classes = [truncate ? 'truncate' : '', lines ? 'line-clamp' : '', className ?? ''].filter(Boolean).join(' ')
  return (
    <Tag className={classes || undefined} style={{ ...type, color: tone && `var(--${tone})`, WebkitLineClamp: lines, ...(grow ? { flex: 1, minWidth: 0 } : {}), fontVariantNumeric: tabular ? 'tabular-nums' : undefined, ...style }} {...rest}>
      {children}
    </Tag>
  )
}
