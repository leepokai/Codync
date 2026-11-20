/** Fills a template's `{key}`s in one pass: a device named "{code}" stays as written. */
export const fill = (template: string, values: Record<string, string>) =>
  template.replace(/\{(\w+)\}/g, (match, key: string) => values[key] ?? match)

/** Fills a translated phrase's `%d` placeholders in order. */
export const counted = (text: string, ...counts: number[]) => counts.reduce((s, n) => s.replace('%d', String(n)), text)
