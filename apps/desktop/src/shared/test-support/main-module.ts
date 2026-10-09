import { readFileSync } from 'node:fs'
import { createContext, Script } from 'node:vm'
import ts from 'typescript'

/** Runs the actual main-process source with explicit, fail-closed OS boundaries. */
export function loadMainModule<T>(file: string, dependencies: Record<string, unknown>, globals: Record<string, unknown> = {}): T {
  const url = new URL(`../../main/${file}.ts`, import.meta.url)
  const output = ts.transpileModule(readFileSync(url, 'utf8'), {
    compilerOptions: { module: ts.ModuleKind.CommonJS, target: ts.ScriptTarget.ES2023, esModuleInterop: true },
    fileName: url.pathname,
  }).outputText
  const exports = {}
  const context = createContext({
    exports, Error, Promise, console, AbortSignal,
    require: (name: string) => {
      if (!(name in dependencies)) throw new Error(`Unmocked main-process dependency: ${name}`)
      return dependencies[name]
    },
    ...globals,
  })
  new Script(output, { filename: url.pathname }).runInContext(context)
  return exports as T
}

export function deferred<T = void>() {
  let resolve!: (value: T | PromiseLike<T>) => void
  let reject!: (reason: unknown) => void
  const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no })
  return { promise, resolve, reject }
}

export const settle = () => new Promise<void>((resolve) => setImmediate(resolve))
