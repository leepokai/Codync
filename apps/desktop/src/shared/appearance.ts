export const APPEARANCES = [
  { id: 'system', label: 'System' },
  { id: 'light', label: 'Light' },
  { id: 'dark', label: 'Dark' },
] as const

export type Appearance = typeof APPEARANCES[number]['id']

export function appearance(value: unknown): Appearance {
  return value === 'light' || value === 'dark' ? value : 'system'
}
