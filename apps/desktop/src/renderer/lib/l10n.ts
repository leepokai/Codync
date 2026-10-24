import jaPhrases from '../../../../ios/Kit/Sources/CodyncKit/Resources/Languages/ja.json' with { type: 'json' }
import koPhrases from '../../../../ios/Kit/Sources/CodyncKit/Resources/Languages/ko.json' with { type: 'json' }
import frPhrases from '../../../../ios/Kit/Sources/CodyncKit/Resources/Languages/fr.json' with { type: 'json' }
import dePhrases from '../../../../ios/Kit/Sources/CodyncKit/Resources/Languages/de.json' with { type: 'json' }
import esPhrases from '../../../../ios/Kit/Sources/CodyncKit/Resources/Languages/es.json' with { type: 'json' }
import hansPhrases from '../../../../ios/Kit/Sources/CodyncKit/Resources/Languages/zh-Hans.json' with { type: 'json' }
import hantPhrases from '../../../../ios/Kit/Sources/CodyncKit/Resources/Languages/zh-Hant.json' with { type: 'json' }

/** This app's interface language. `zh` is Simplified, kept so a saved choice still means that. */
export const appLanguages = ['system', 'en', 'zh', 'zh-Hant', 'ja', 'ko', 'fr', 'de', 'es'] as const
export type AppLanguage = typeof appLanguages[number]

/** Traditional is its own table. A boolean `true` from older callers means Simplified. */
export type ChineseScript = 'hans' | 'hant'

const hans = hansPhrases as Record<string, string>
const hant = hantPhrases as Record<string, string>

function languageTag(system: string | boolean, fallback = typeof navigator === 'undefined' ? '' : navigator.language): string {
  if (system === true) return 'zh-Hans'
  if (system === false) return 'en'
  return system || fallback
}

/** `zh-Hant`, `zh-TW`, `zh-HK` and `zh-MO` read Traditional. Other Chinese locales read Simplified. */
export function systemScript(language = typeof navigator === 'undefined' ? '' : navigator.language): ChineseScript | null {
  const id = language.toLowerCase().replaceAll('_', '-').split('.')[0] ?? ''
  if (!id.startsWith('zh')) return null
  if (id.includes('hant') || id.includes('-tw') || id.includes('-hk') || id.includes('-mo')) return 'hant'
  return 'hans'
}

export function systemIsChinese(language = typeof navigator === 'undefined' ? '' : navigator.language): boolean {
  return systemScript(language) != null
}

/** The table this choice reads. A boolean `true` from older callers means Simplified. */
export function resolvedScript(preference: AppLanguage, system: string | boolean): ChineseScript | null {
  if (preference === 'zh') return 'hans'
  if (preference === 'zh-Hant') return 'hant'
  if (preference !== 'system') return null
  return systemScript(languageTag(system))
}

export function usesChinese(preference: AppLanguage, system: string | boolean): boolean {
  return resolvedScript(preference, system) != null
}

export function resolvedLanguage(preference: AppLanguage, system: string | boolean): Exclude<AppLanguage, 'system'> {
  if (preference !== 'system' && appLanguages.includes(preference)) return preference
  const tag = languageTag(system).toLowerCase().replaceAll('_', '-').split('.')[0]
  const base = tag.split('-')[0]
  if (base === 'zh') return systemScript(tag) === 'hant' ? 'zh-Hant' : 'zh'
  return ['ja', 'ko', 'fr', 'de', 'es'].includes(base) ? base as 'ja' | 'ko' | 'fr' | 'de' | 'es' : 'en'
}

const catalogs: Record<Exclude<AppLanguage, 'system'>, Record<string, string>> = {
  en: {}, zh: hans, 'zh-Hant': hant, ja: jaPhrases, ko: koPhrases, fr: frPhrases, de: dePhrases, es: esPhrases,
}

/** One piece of a phrase key such as "Runs the official installer on {name}." */
interface Piece { literal?: string; token?: string }

/**
 * Phrase keys that already contain `{token}` slots, longest fixed text first.
 * A filled sentence ("Runs the official installer on this Mac.") matches the key,
 * and a slot that is itself a known phrase (a setup hint) is translated too.
 */
const templates: Array<{ key: string; pieces: Piece[]; literal: number }> = Object.keys(hans).flatMap((key) => {
  const pieces = parseTemplate(key)
  if (!pieces) return []
  const literal = pieces.reduce((n, piece) => n + (piece.literal?.length ?? 0), 0)
  return literal >= 6 ? [{ key, pieces, literal }] : []
}).sort((a, b) => b.literal - a.literal || b.key.length - a.key.length)

function parseTemplate(template: string): Piece[] | undefined {
  const pieces: Piece[] = []
  let rest = template
  while (rest.includes('{')) {
    const open = rest.indexOf('{')
    if (open > 0) pieces.push({ literal: rest.slice(0, open) })
    const close = rest.indexOf('}', open)
    if (close < 0) return undefined
    const token = rest.slice(open, close + 1)
    const name = token.slice(1, -1)
    if (name.length === 0 || !/^[A-Za-z0-9]+$/.test(name) || pieces.at(-1)?.token) return undefined
    pieces.push({ token })
    rest = rest.slice(close + 1)
  }
  if (rest.length > 0) pieces.push({ literal: rest })
  return pieces.some((piece) => piece.token) ? pieces : undefined
}

function hit(key: string, primary: Record<string, string>, fallback: Record<string, string>): string | undefined {
  return primary[key] ?? fallback[key]
}

function fill(text: string, pieces: Piece[], phrase: string, primary: Record<string, string>, fallback: Record<string, string>, depth: number): string | undefined {
  let pos = 0
  const caps = new Map<string, string>()
  for (let i = 0; i < pieces.length; i++) {
    const piece = pieces[i]
    if (piece.literal !== undefined) {
      if (!text.startsWith(piece.literal, pos)) return undefined
      pos += piece.literal.length
      continue
    }
    const token = piece.token ?? ''
    const next = pieces[i + 1]?.literal
    const end = next === undefined ? text.length : text.indexOf(next, pos)
    if (end < 0 || end === pos) return undefined
    caps.set(token, text.slice(pos, end))
    pos = end
  }
  if (pos !== text.length) return undefined
  let result = phrase
  for (const [token, value] of caps) {
    const inner = hit(value, primary, fallback) ?? (depth === 0 ? templated(value, primary, fallback, 1) : undefined) ?? value
    result = result.replaceAll(token, inner)
  }
  return result
}

/** A sentence the host or a view already filled in, matched back to its `{token}` phrase. */
function templated(text: string, primary: Record<string, string>, fallback: Record<string, string>, depth = 0): string | undefined {
  for (const template of templates) {
    const phrase = hit(template.key, primary, fallback)
    const filled = phrase ? fill(text, template.pieces, phrase, primary, fallback, depth) : undefined
    if (filled) return filled
  }
  return undefined
}

function phrase(text: string, primary: Record<string, string>, fallback: Record<string, string>): string | undefined {
  return hit(text, primary, fallback) ?? templated(text, primary, fallback)
}

/** Exact chrome phrases, including a label whose JSX kept the surrounding spaces. */
export function translate(text: string, preference: AppLanguage, system: string | boolean = 'en'): string {
  const language = resolvedLanguage(preference, system)
  if (language === 'en') return text
  const primary = catalogs[language]
  const fallback = language === 'zh-Hant' ? hans : {}
  const direct = phrase(text, primary, fallback)
  if (direct) return direct
  const trimmed = text.trim()
  return trimmed === text ? text : (phrase(trimmed, primary, fallback) ?? text)
}

/** "resets 3h 20m" / "resets in 2d 6h". Compact times become "3小时20分后重置". */
export function resetLine(raw: string, preference: AppLanguage, system: string | boolean = 'en'): string {
  const time = raw.startsWith('resets in ') ? raw.slice('resets in '.length) : raw.startsWith('resets ') ? raw.slice('resets '.length) : null
  if (time == null) return translate(raw, preference, system)
  const language = resolvedLanguage(preference, system)
  if (language === 'en') return raw
  if (language !== 'zh' && language !== 'zh-Hant') {
    return /^[0-9]+[dhm]( [0-9]+[dhm])*$/.test(time)
      ? translate('resets in {time}', preference, system).replaceAll('{time}', time)
      : `${translate('resets', preference, system)} ${time}`
  }
  const spoken = compactTime(time, (key) => translate(key, preference, system))
  return spoken ? spoken + translate('after reset', preference, system) : `${translate('resets', preference, system)} ${time}`
}

function compactTime(time: string, tr: (key: string) => string): string | null {
  let spoken = ''
  let matched = false
  for (const token of time.split(' ')) {
    const unit = token.at(-1)
    const n = Number(token.slice(0, -1))
    if (!unit || token.length < 2 || !Number.isInteger(n)) return null
    const word = unit === 'd' ? tr('day unit') : unit === 'h' ? tr('hour unit') : unit === 'm' ? tr('minute unit') : null
    if (!word) return null
    matched = true
    if (n === 0) continue
    spoken += `${n}${word}`
  }
  if (!matched) return null
  return spoken || `0${tr('minute unit')}`
}

const nativeNames: Record<Exclude<AppLanguage, 'system'>, string> = {
  en: 'English', zh: '简体中文', 'zh-Hant': '繁體中文', ja: '日本語', ko: '한국어', fr: 'Français', de: 'Deutsch', es: 'Español',
}

export function languageTitle(language: AppLanguage, preference: AppLanguage, system: string | boolean): string {
  return language === 'system' ? translate('Follow system', preference, system) : nativeNames[language]
}
