/** Where the window has been, for Back and Forward: the places visited in order, and the one shown. */
export interface Visits<T> {
  places: T[]
  at: number
}

export const noVisits = <T>(): Visits<T> => ({ places: [], at: -1 })

/** The window shows `place`. Nothing new when the visits already stand there (where Back and Forward land); else it
 * follows the place shown, and what lay ahead of that is dropped. The last 50 are kept. */
export function visit<T>(visits: Visits<T>, place: T, same: (a: T, b: T) => boolean): Visits<T> {
  const here = visits.places[visits.at]
  if (here !== undefined && same(here, place)) return visits
  const places = [...visits.places.slice(0, visits.at + 1), place].slice(-50)
  return { places, at: places.length - 1 }
}

/** One place back (-1) or forward (1); null at either end. */
export const step = <T>(visits: Visits<T>, by: -1 | 1): Visits<T> | null => (visits.places[visits.at + by] === undefined ? null : { ...visits, at: visits.at + by })
