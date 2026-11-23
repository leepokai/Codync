import { test } from '@playwright/test'
import { api, attach } from './sandbox.ts'

type MirrorBot = { id: string; name: string }

/** One-off: delete the e2e leftovers previous runs of model-manager.spec.ts left behind, read from the app's own mirror. */
test('clean model-manager e2e leftovers', async () => {
  test.setTimeout(120_000)
  const { browser, page } = await attach()
  try {
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
    const leftovers = rows.filter((b) => b.name.startsWith('Model manager e2e'))
    console.log('leftovers:', leftovers.map((b) => b.name).join(', '))
    for (const bot of leftovers) {
      await api(page, 'deleteBot', { botId: bot.id }).catch((e) => console.log('delete failed', bot.name, String(e)))
    }
  } finally {
    await browser.close()
  }
})
