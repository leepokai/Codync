import { existsSync } from 'node:fs'

/** Lets the desktop tests import extensionless TypeScript and `@shared/` on Node's module.register. */
export function resolve(specifier, context, nextResolve) {
  if (specifier.startsWith('@shared/')) {
    return nextResolve(new URL(`../../shared/${specifier.slice(8)}.ts`, import.meta.url).href, context)
  }
  if (specifier.startsWith('.') && context.parentURL && !/\.[a-z]+$/.test(specifier)) {
    const url = new URL(`${specifier}.ts`, context.parentURL)
    if (existsSync(url)) return nextResolve(url.href, context)
  }
  return nextResolve(specifier, context)
}
