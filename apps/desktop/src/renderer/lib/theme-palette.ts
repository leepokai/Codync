/** A theme palette from the person's theme files, worn instead of the app's own colors. */

/** Palette key → the CSS variables it sets. `accent` is the readable ink; `color4` fills. */
const MAP: Record<string, string[]> = {
  background: ['--background'],
  foreground: ['--text'],
  color0: ['--surface'],
  color8: ['--border'],
  color4: ['--accent', '--accent-fill', '--bubble-user'],
  color7: ['--secondary'],
  color6: ['--tertiary'],
}

const applied = new Set<string>()

/** Clears the previous palette, then writes `palette` (an empty palette restores the app's own). */
export function applyThemePalette(palette: Record<string, string>) {
  const root = document.documentElement.style
  for (const name of applied) root.removeProperty(name)
  applied.clear()
  for (const [key, vars] of Object.entries(MAP)) {
    const color = palette[key]
    if (!color) continue
    for (const name of vars) {
      root.setProperty(name, color)
      applied.add(name)
    }
  }
}
