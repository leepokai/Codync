/** Device-local section order. The empty name stands for the unsectioned Bots group. */
export function orderedSections(names: readonly string[], saved: readonly string[] = []): string[] {
  return [...new Set([...saved.filter((name) => names.includes(name)), ...names])]
}

/** Keep a renamed section in its place; a merge keeps the destination's existing place. */
export function renamedSectionOrder(order: readonly string[], from: string, to: string | null): string[] {
  if (from === to) return [...order]
  return order.flatMap((name) => name !== from ? [name] : to === null || order.includes(to) ? [] : [to])
}
