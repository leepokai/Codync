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
    // model-manager.spec.ts names its bots "Bot manager e2e ..."; the old "Model manager e2e"
    // prefix stays for leftover junk from pre-rename runs.
    const leftovers = rows.filter((b) => /^(Model|Bot) manager e2e/.test(b.name))
    console.log('leftovers:', leftovers.map((b) => b.name).join(', '))
    for (const bot of leftovers) {
      await api(page, 'deleteBot', { botId: bot.id }).catch((e) => console.log('delete failed', bot.name, String(e)))
    }
    // The cleanup must not report green without having done its work: after the deletions,
    // no e2e leftovers of either prefix may remain. The mirror is a cache that trails the
    // host (the roster echo can lag, like every other wait in these specs), so poll for it
    // with the same 30s grace the positives get; only a leftover that survives it fails.
    const deadline = Date.now() + 30_000
    for (;;) {
      const rest = await page.evaluate((): MirrorBot[] => {
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
      const remaining = rest.filter((b) => /^(Model|Bot) manager e2e/.test(b.name))
      if (remaining.length === 0) break
      if (Date.now() > deadline) throw new Error(`cleanup left e2e bots behind: ${remaining.map((b) => b.name).join(', ')}`)
      await page.waitForTimeout(1_000)
    }
  } finally {
    await browser.close()
  }
})
