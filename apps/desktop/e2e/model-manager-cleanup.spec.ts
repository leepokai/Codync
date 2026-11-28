import { expect, test } from '@playwright/test'
import { api, attach } from './sandbox.ts'

type MirrorBot = { id: string; name: string }

/** The names `model-manager.spec.ts` gives the bots it creates; a run that died before its `finally` left one behind. */
const LEFTOVER = /^(?:Bot manager e2e [AB]|Provider (?:custom|agent)) \d+$/

/**
 * One-off: delete the bots previous runs of model-manager.spec.ts left behind, then check from the app's
 * own mirror that none of them is still there — a cleanup that silently fails would keep feeding the
 * Bot Manager rows the later runs trip over.
 */
test('clean model-manager e2e leftovers', async () => {
  test.setTimeout(120_000)
  const { browser, page } = await attach()
  try {
    const leftovers = async (): Promise<MirrorBot[]> => {
      const rows = await page.evaluate((): MirrorBot[] => {
        const keys = Object.keys(localStorage).filter((k) => k.startsWith('codync-mirror-local'))
        const seen = new Map<string, MirrorBot>()
        for (const k of keys) {
          try {
            const parsed = JSON.parse(localStorage.getItem(k) ?? '{}') as { bots?: MirrorBot[] }
            for (const bot of parsed.bots ?? []) if (bot.id && bot.name) seen.set(bot.id, bot)
          } catch {
            // A half-written mirror line is not worth failing over.
          }
        }
        return [...seen.values()]
      })
      return rows.filter((b) => LEFTOVER.test(b.name))
    }

    const found = await leftovers()
    test.info().annotations.push({ type: 'leftovers', description: found.map((b) => b.name).join(', ') || 'none' })
    for (const bot of found) await api(page, 'deleteBot', { botId: bot.id })

    // The app's mirror catches up with the deletions it hears about a moment later.
    await expect.poll(async () => (await leftovers()).map((b) => b.name), { timeout: 15_000 }).toEqual([])
  } finally {
    await browser.close()
  }
})
