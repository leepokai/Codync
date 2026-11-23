/** A bot on a computer: bot ids are only unique per computer. */
export interface BotRef {
  computerId: string
  botId: string
}

export const refKey = (r: BotRef) => `${r.computerId}/${r.botId}`
export const parseRef = (key: string): BotRef | null => {
  const at = key.indexOf('/')
  return at < 0 ? null : { computerId: key.slice(0, at), botId: key.slice(at + 1) }
}
export const sameRef = (a: BotRef | null, b: BotRef | null) => !!a && !!b && a.computerId === b.computerId && a.botId === b.botId
