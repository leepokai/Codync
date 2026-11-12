#!/usr/bin/env node
// Regenerates Codync's connector catalogue (host/src/market/copilotkit/apps.json) from a
// CopilotKit/OpenBot checkout. Run it from the repository root:
//
//   node tools/import-copilotkit-connectors.mjs ../copilotkit-openbot
//
// What comes from upstream (`server/src/plugins/catalogue.ts`, reviewed there): every entry, its
// key, title, vendor, host and path, auth kind, header, OAuth endpoints, scopes, consent
// parameters, registration endpoint, write/read tool lists and docs link.
//
// What Codync writes itself: the presentation — category, tagline, description, website, featured,
// prompts, field labels and hints. Those are kept from the file being replaced, matched by app id,
// so regenerating never loses them; a new upstream entry is written with its upstream summary as a
// placeholder tagline and no prompts, to be written by hand before commit.
//
// The output is committed. Re-running is safe and idempotent.
import { readFileSync, writeFileSync } from 'node:fs'
import { resolve } from 'node:path'

const source = process.argv[2]
if (!source) {
    console.error('usage: node tools/import-copilotkit-connectors.mjs <copilotkit-openbot checkout>')
    process.exit(1)
}

const catalogueFile = resolve(source, 'server/src/plugins/catalogue.ts')
const out = resolve('host/src/market/copilotkit/apps.json')
const today = new Date().toISOString().slice(0, 10)

/**
 * Where Codync differs from upstream on purpose, and only where it must.
 *
 * Upstream reaches Google over its own REST adapter (`google-drive-rest`), which exists inside
 * OpenBot and nowhere else; Codync speaks MCP, so the entry points at Google's MCP server instead.
 * The host and path are Google's own (same vendor, same credential, same tools); enrolling a Cloud
 * project in the Workspace Developer Preview is the price, and the sheet says so.
 */
const OVERRIDES = {
    'google-drive': {
        host: 'https://drivemcp.googleapis.com',
        path: '/mcp/v1',
        transport: 'http',
    },
}

const text = readFileSync(catalogueFile, 'utf8')

/** A top-level `const NAME = Object.freeze([...])` in the file, for the lists that use one. */
const constants = new Map()
for (const m of text.matchAll(/^const (\w+) = Object\.freeze\(\[([\s\S]*?)\]\);/gm)) {
    constants.set(m[1], strings(m[2]))
}

/** The quoted strings of a list literal, or the ones of the constant it names. */
function strings(list) {
    if (!list) return []
    if (!list.includes('"')) return constants.get(list.trim()) ?? []
    return [...list.matchAll(/"([^"]*)"/g)].map((m) => m[1])
}

/** The strings after `name:`: an `Object.freeze([...])` literal, or a constant naming one. */
function listAfter(one, name) {
    const at = new RegExp(`^\\s*${name}:\\s*`, 'm').exec(one)
    if (!at) return []
    const rest = one.slice(at.index + at[0].length)
    const frozen = /^Object\.freeze\(\[([\s\S]*?)\]\)/.exec(rest)
    if (frozen) return strings(frozen[1])
    const named = /^\w+/.exec(rest)?.[0]
    return strings(named)
}

/** The `{...}` block after `key:`, by brace counting (upstream nests them in `auth`). */
function block(source, from) {
    const start = source.indexOf('{', from)
    let depth = 0
    for (let i = start; i < source.length; i++) {
        if (source[i] === '{') depth++
        else if (source[i] === '}' && --depth === 0) return source.slice(start, i + 1)
    }
    throw new Error('unbalanced block')
}

/** The entries of the `CATALOGUE` array, each one's own object literal. */
function entries() {
    const at = text.indexOf('export const CATALOGUE')
    if (at < 0) throw new Error('no CATALOGUE in the catalogue file')
    const start = text.indexOf('Object.freeze([', at) + 'Object.freeze('.length
    if (start < 'Object.freeze('.length) throw new Error('CATALOGUE is not a frozen array')
    let depth = 0
    let end = start
    for (let i = start; i < text.length; i++) {
        if (text[i] === '[') depth++
        else if (text[i] === ']' && --depth === 0) {
            end = i
            break
        }
    }
    const list = text.slice(start + 1, end)
    const found = []
    let i = 0
    while ((i = list.indexOf('{', i)) >= 0) {
        const one = block(list, i)
        found.push(one)
        i += one.length
    }
    return found
}

const field = (one, name) =>
    new RegExp(`(?:^|[{,\\s])${name}:\\s*(?:\\n\\s*)?"([^"]*)"`, 'm').exec(one)?.[1]

function auth(one) {
    const at = one.indexOf('auth:')
    if (at < 0) throw new Error('entry without auth')
    const body = block(one, at)
    const kind = field(body, 'kind')
    const scopes = listAfter(body, 'scopes')
    const params = {}
    const pin = /authorizationParams:\s*Object\.freeze\(\{([\s\S]*?)\}\)/.exec(body)?.[1]
    for (const m of (pin ?? '').matchAll(/(\w+):\s*"([^"]*)"/g)) params[m[1]] = m[2]
    const authorizationUrl = field(body, 'authorizationUrl')
    return {
        kind,
        header: field(body, 'header'),
        client: field(body, 'clientRegistration') === 'dynamic' ? 'dynamic' : kind === 'user-oauth' ? 'pasted' : null,
        // The issuer is the authorization server itself: the consent must come back from it.
        issuer: authorizationUrl ? new URL(authorizationUrl).origin : null,
        authorizationUrl,
        tokenUrl: field(body, 'tokenUrl'),
        revokeUrl: field(body, 'revokeUrl'),
        registrationUrl: field(body, 'registrationUrl'),
        scopes,
        authorizationParams: params,
    }
}

/** The sheet's key fields: one for a deployment key, a client pair for a pasted OAuth client. */
function fields(entry, shown) {
    const keep = (id, field) => ({
        ...field,
        ...pick(shown.fields?.find((f) => f.id === id) ?? {}, ['label', 'placeholder', 'hint']),
    })
    if (entry.auth.kind === 'deployment-bearer') {
        const header = entry.auth.header ?? 'Authorization'
        return [
            keep('apiKey', {
                id: 'apiKey',
                label: `${entry.creator} API key`,
                header,
                template: header === 'Authorization' ? 'Bearer {apiKey}' : '{apiKey}',
                url: false,
                placeholder: 'Your API key',
                hint: 'Saved on this computer only.',
                required: true,
            }),
        ]
    }
    if (entry.auth.kind === 'user-oauth' && entry.auth.client === 'pasted') {
        return [
            keep('clientId', {
                id: 'clientId',
                label: 'OAuth client ID',
                url: false,
                placeholder: 'Your client ID',
                hint: 'Saved on this computer only.',
                required: true,
            }),
            keep('clientSecret', {
                id: 'clientSecret',
                label: 'OAuth client secret',
                url: false,
                placeholder: 'Your client secret',
                hint: 'Saved on this computer only.',
                required: true,
            }),
        ]
    }
    return []
}

function pick(from, keys) {
    const out = {}
    for (const key of keys) if (from[key] != null) out[key] = from[key]
    return out
}

function readShown() {
    try {
        return new Map((JSON.parse(readFileSync(out, 'utf8')).apps ?? []).map((app) => [app.id, app]))
    } catch {
        return new Map()
    }
}

const previous = readShown()

const apps = entries().map((one) => {
    const key = field(one, 'key')
    const vendor = field(one, 'vendor')
    const upstream = {
        id: key,
        name: field(one, 'title'),
        creator: vendor,
        auth: auth(one),
        transport: field(one, 'transport') === 'builtin-routines' ? 'builtin' : 'http',
        host: field(one, 'host'),
        path: field(one, 'path'),
    }
    const shown = previous.get(key) ?? {}
    const override = OVERRIDES[key] ?? {}
    const builtin = upstream.auth.kind === 'builtin'
    const authOut = { kind: upstream.auth.kind }
    for (const name of [
        'header',
        'client',
        'issuer',
        'authorizationUrl',
        'tokenUrl',
        'revokeUrl',
        'registrationUrl',
    ]) {
        if (upstream.auth[name]) authOut[name] = upstream.auth[name]
    }
    for (const name of ['scopes', 'authorizationParams']) {
        if (Object.keys(upstream.auth[name]).length > 0) authOut[name] = upstream.auth[name]
    }
    return {
        id: key,
        name: upstream.name,
        tagline: shown.tagline ?? `${upstream.name} as a connector.`,
        description: shown.description ?? field(one, 'summary') ?? '',
        category: shown.category ?? 'productivity',
        creator: vendor,
        version: shown.version ?? '1.0.0',
        updated: today,
        ...(shown.website ? { website: shown.website } : {}),
        featured: shown.featured === true,
        setup: {
            none: 'open',
            builtin: 'open',
            'deployment-bearer': 'key',
            'user-oauth': 'link',
        }[authOut.kind] ?? 'local',
        ...(builtin ? { builtin: true } : {}),
        server: {
            name: key,
            transport: override.transport ?? upstream.transport,
            url: builtin ? null : `${override.host ?? upstream.host}${override.path ?? upstream.path}`,
        },
        auth: authOut,
        writeTools: listAfter(one, 'writeTools'),
        readTools: listAfter(one, 'readTools'),
        fields: fields(upstream, shown),
        steps: shown.steps ?? [],
        docsUrl: field(one, 'docsUrl'),
        docsLabel: shown.docsLabel ?? (authOut.kind === 'deployment-bearer' ? 'Get a key' : 'Open the guide'),
        prompts: shown.prompts ?? [],
    }
})

if (apps.length === 0) throw new Error('no entries found')
writeFileSync(out, JSON.stringify({ apps }, null, 2) + '\n')
console.log(`wrote ${apps.length} connectors to ${out}`)
