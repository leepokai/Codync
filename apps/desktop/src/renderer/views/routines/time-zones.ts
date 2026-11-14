// Time zones for the routine editor's picker: every IANA zone the runtime knows, each with its
// offset right now, and a forgiving match on what's typed.

export interface TimeZoneOption {
  id: string
  /** "GMT+8", "GMT-4:30", "GMT" */
  offset: string
}

/** A zone's offset from UTC right now, as the runtime names it ("GMT+8"); empty for an unknown zone. */
export function zoneOffset(zone: string, now = new Date()) {
  try {
    const parts = new Intl.DateTimeFormat('en-US', { timeZone: zone, timeZoneName: 'shortOffset' }).formatToParts(now)
    return parts.find((p) => p.type === 'timeZoneName')?.value ?? ''
  } catch {
    return ''
  }
}

// ICU (Chromium, Node) still lists these zones by retired names; IANA, the host and the phones
// use the current ones.
const renamed: Record<string, string> = {
  'Africa/Asmera': 'Africa/Asmara',
  'America/Buenos_Aires': 'America/Argentina/Buenos_Aires',
  'America/Catamarca': 'America/Argentina/Catamarca',
  'America/Coral_Harbour': 'America/Atikokan',
  'America/Cordoba': 'America/Argentina/Cordoba',
  'America/Godthab': 'America/Nuuk',
  'America/Indianapolis': 'America/Indiana/Indianapolis',
  'America/Jujuy': 'America/Argentina/Jujuy',
  'America/Louisville': 'America/Kentucky/Louisville',
  'America/Mendoza': 'America/Argentina/Mendoza',
  'Asia/Calcutta': 'Asia/Kolkata',
  'Asia/Katmandu': 'Asia/Kathmandu',
  'Asia/Rangoon': 'Asia/Yangon',
  'Asia/Saigon': 'Asia/Ho_Chi_Minh',
  'Atlantic/Faeroe': 'Atlantic/Faroe',
  'Europe/Kiev': 'Europe/Kyiv',
  'Pacific/Enderbury': 'Pacific/Kanton',
  'Pacific/Ponape': 'Pacific/Pohnpei',
  'Pacific/Truk': 'Pacific/Chuuk',
}

/** A zone by its current IANA name ("Asia/Calcutta" → "Asia/Kolkata"). */
export const currentZone = (id: string) => renamed[id] ?? id

export const timeZoneOptions = (now = new Date()): TimeZoneOption[] =>
  ['UTC', ...[...new Set(Intl.supportedValuesOf('timeZone').map(currentZone))].sort()].map((id) => ({ id, offset: zoneOffset(id, now) }))

/** "America/Argentina/Buenos_Aires" → "America/Argentina/Buenos Aires" */
export const zoneLabel = (id: string) => id.replaceAll('_', ' ')

const words = (text: string) => text.toLowerCase().split(/[\s/_-]+/).filter(Boolean)

/** How well a zone's words match the typed ones (lower is better), or null when they don't. */
function rank(name: string[], typed: string[]): number | null {
  const joined = name.join(' ')
  if (joined.startsWith(typed.join(' '))) return 0
  if (typed.every((word) => name.some((part) => part.startsWith(word)))) return 1
  if (typed.every((word) => joined.includes(word))) return 2
  let at = 0
  const letters = name.join('')
  for (const letter of typed.join('')) {
    at = letters.indexOf(letter, at) + 1
    if (at === 0) return null
  }
  return 3
}

/**
 * The zones matching what's typed, best first (as on the phones): the name starts with it, then
 * every typed word starts a word ("new york", "asia tai"), then every word is inside it ("aipe"),
 * then its letters appear in order ("tpe"). Ties keep their order.
 */
export function matchZones(zones: TimeZoneOption[], query: string) {
  const typed = words(query)
  if (!typed.length) return zones
  return zones
    .map((zone) => [zone, rank(words(zone.id), typed)] as const)
    .filter((match): match is readonly [TimeZoneOption, number] => match[1] !== null)
    .sort((a, b) => a[1] - b[1])
    .map(([zone]) => zone)
}
