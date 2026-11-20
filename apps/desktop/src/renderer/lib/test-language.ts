import type { TestContext } from 'node:test'

/** Give a test a known browser language and restore the host's actual navigator afterwards. */
export function testLanguage(t: TestContext, language: string) {
  const original = Object.getOwnPropertyDescriptor(globalThis, 'navigator')
  t.after(() => {
    if (original) Object.defineProperty(globalThis, 'navigator', original)
    else Reflect.deleteProperty(globalThis, 'navigator')
  })
  Object.defineProperty(globalThis, 'navigator', { configurable: true, value: { language } })
}
