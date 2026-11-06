#!/usr/bin/env node
// Imports the OpenBot marketplace content (agents, skills, apps) into the host's bundled
// marketplace catalogs. Run it from the repository root with the OpenBot checkout as the argument:
//
//   node tools/import-openbot-catalog.mjs ../openbot-nightly
//
// The output is committed. Re-running is safe: the generated folders are replaced.
import { execFileSync } from 'node:child_process'
import { cpSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'

const source = resolve(process.argv[2] ?? '')
if (!process.argv[2]) {
    console.error('usage: node tools/import-openbot-catalog.mjs <openbot checkout>')
    process.exit(1)
}

const out = resolve('host/src/market/openbot')
const production = join(source, 'marketplace', 'production-catalog')
const plugins = join(source, 'marketplace', 'plugin-catalog', 'plugins')

/** Codync avatar colors, in the order a hue runs through them. */
const COLORS = [
    [20, 'red'],
    [45, 'orange'],
    [70, 'yellow'],
    [160, 'green'],
    [195, 'cyan'],
    [240, 'blue'],
    [280, 'violet'],
    [330, 'magenta'],
    [360, 'red'],
]
const SHAPES = ['blob', 'pebble', 'squircle', 'tablet', 'wedge', 'hex', 'cloud', 'teardrop']

const colorFor = (hue) => COLORS.find(([end]) => hue < end)?.[1] ?? 'gray'
const shapeFor = (slug) => {
    let hash = 0
    for (const char of slug) hash = (hash * 31 + char.charCodeAt(0)) >>> 0
    return SHAPES[hash % SHAPES.length]
}

/** A stable id for a skill's source: the slug OpenBot uses, in lower case. */
const skillId = (slug) => slug.toLowerCase()

const catalogFile = join(production, 'catalog.json')
const catalog = JSON.parse(readFileSync(catalogFile, 'utf8'))
const catalogUpdated = statSync(catalogFile).mtime.toISOString().slice(0, 10)

// MARK: skills
rmSync(join(out, 'skills'), { recursive: true, force: true })
mkdirSync(join(out, 'skills'), { recursive: true })

/** The instructions of a SKILL.md without its front matter, for the details sheet. */
const body = (md) => md.replace(/^---[\s\S]*?\n---\n/, '').trim()

const skills = catalog.skills.map((skill) => {
    const dir = join(production, 'skills', skill.slug)
    const copy = join(out, 'skills', skillId(skill.slug))
    cpSync(dir, copy, { recursive: true })
    const md = readFileSync(join(dir, 'SKILL.md'), 'utf8')
    return {
        id: skillId(skill.slug),
        name: /^name:\s*(.+)$/m.exec(md)?.[1]?.replace(/^"|"$/g, '') ?? skill.slug,
        description: /^description:\s*(.+)$/m.exec(md)?.[1]?.replace(/^"|"$/g, '') ?? '',
        category: skill.category,
        source: skill.source,
        featured: skill.featured === true,
        body: body(md),
    }
})

// The pstack pack ships 52 skills, each prefixed as OpenBot prefixes them.
const pstack = join(production, 'bundles', 'pstack')
const pstackChildren = []
for (const dir of readdirSync(join(pstack, 'skills'))) {
    const id = `pstack-${skillId(dir)}`
    cpSync(join(pstack, 'skills', dir), join(out, 'skills', id), { recursive: true })
    const md = readFileSync(join(pstack, 'skills', dir, 'SKILL.md'), 'utf8')
    pstackChildren.push(id)
    skills.push({
        id,
        name: `pstack: ${/^name:\s*(.+)$/m.exec(md)?.[1]?.replace(/^"|"$/g, '') ?? dir}`,
        description: /^description:\s*(.+)$/m.exec(md)?.[1]?.replace(/^"|"$/g, '') ?? '',
        category: 'coding',
        source: 'cursor',
        featured: false,
        pack: 'pstack',
        body: body(md),
    })
}
cpSync(join(pstack, 'LICENSE'), join(out, 'skills', 'pstack-poteto-mode', 'LICENSE'))
cpSync(join(pstack, 'NOTICE.md'), join(out, 'skills', 'pstack-poteto-mode', 'NOTICE.md'))

const packs = catalog.packs.map((pack) => ({
    id: pack.slug,
    name: pack.slug,
    description: pack.description,
    category: pack.category,
    children: pstackChildren,
}))

// MARK: agents
const agents = catalog.agents.map((agent) => ({
    id: agent.slug,
    name: agent.name,
    title: agent.title,
    instructions: agent.description,
    category: agent.category,
    creator: 'OpenBot',
    version: 4,
    updated: catalogUpdated,
    avatarColor: colorFor(agent.avatarHue ?? 210),
    avatarShape: shapeFor(agent.slug),
    featured: agent.featured === true,
    skills: agent.skills.map(skillId),
    routines: (agent.routines ?? []).map((routine) => ({
        name: routine.name,
        instruction: routine.instruction,
        active: routine.active === true,
        schedule: routine.schedule,
    })),
}))

// A Codync-native extra: OpenBot's workflow playbook, wired to Codync's routine canvas and tools.
agents.push({
    id: 'workflow-builder',
    name: 'Workflow Builder',
    title: 'A trigger, ordered steps and a result, drawn on the canvas as it is built',
    instructions: 'You are Workflow Builder. Your one job: turn a repeated job into a Codync workflow the user can watch and run. A workflow is a routine with one trigger and a chain of steps: every step is another bot receiving the previous result, and the canvas shows each one as it is linked. Inputs: the job, its trigger (a clock, a webhook, a watched file or URL, or a source event), the bots that do each step and where the final result goes. Ask only for what is missing.\n\nHow you work: use the Workflow Playbook skill. First check list_bots and the available routines tools. Create the routine with save_routine and the narrowest trigger: cron with an explicit timezone for clock work, webhook for a service that can post, watch for a changed file or URL, event when a configured sender delivers one. Write one instruction per step that names its input and its named result, never "continue" or "handle it". Link each step with link_routine_bots, giving the downstream bot what it should do with the reply; chain the next bot with fromBotId. Keep the chain short and acyclic. Use list_routine_flow to confirm what was linked, then run_routine once with a test payload when the user agrees; steps show on the canvas in order. For a webhook, get routine_webhook and tell the user the URL, key and content type to set in the sending service, and say the workflow is awaiting connection until a real delivery arrives.\n\nOutput: the routine name, its trigger in plain words, each step with its bot, where the result goes, and what is still to connect. Report what the tools really saved; never claim a run or a delivery that did not happen.\n\nNever: delete or rewire another bot\'s routines without the user saying so, put secrets in instructions, or promise a trigger a sender has not been set up for. A one-off task is a routine, not a workflow: say so. Voice: plain, ordered, exact.',
    category: 'automation',
    creator: 'OpenBot',
    version: 1,
    updated: catalogUpdated,
    avatarColor: 'cyan',
    avatarShape: 'hex',
    featured: true,
    skills: ['workflow-playbook'],
    routines: [],
})

// MARK: apps
const apps = []
for (const slug of readdirSync(plugins)) {
    const plugin = JSON.parse(readFileSync(join(plugins, slug, 'plugin.json'), 'utf8'))
    const pluginUpdated = statSync(join(plugins, slug, 'plugin.json')).mtime.toISOString().slice(0, 10)
    const app = plugin.apps?.[0]
    if (!app) continue
    const auth = app.server.auth?.[0]
    const setup = !auth ? 'open' : auth.kind === 'link' ? 'link' : auth.kind === 'key' ? 'key' : 'local'
    apps.push({
        id: plugin.slug,
        name: plugin.name,
        tagline: plugin.tagline,
        description: plugin.description,
        category: plugin.category,
        creator: plugin.creatorName,
        version: plugin.version,
        updated: pluginUpdated,
        icon: plugin.iconUrl,
        website: plugin.websiteUrl,
        featured: ['aave', 'canva', 'notion', 'figma'].includes(plugin.slug),
        setup,
        server: {
            name: app.server.name,
            transport: app.server.transport,
            url: app.server.url,
            command: app.server.command,
            args: app.server.args,
        },
        fields: (auth?.fields ?? []).map((field) => ({
            id: field.id,
            label: field.label,
            env: field.env,
            header: field.header,
            url: field.url === true,
            placeholder: field.placeholder,
            hint: field.hint,
            required: field.id !== 'url',
        })),
        steps: auth?.steps ?? [],
        docsUrl: auth?.docsUrl,
        docsLabel: auth?.docsLabel,
        prompts: plugin.prompts ?? [],
    })
}

mkdirSync(out, { recursive: true })
// The host embeds this archive: a packaged binary has no source folders beside it.
execFileSync('tar', ['-czf', join(out, 'skills.tar.gz'), '-C', out, 'skills'])
writeFileSync(join(out, 'skills.json'), JSON.stringify({ skills, packs }, null, 2) + '\n')
writeFileSync(join(out, 'agents.json'), JSON.stringify({ agents }, null, 2) + '\n')
writeFileSync(join(out, 'apps.json'), JSON.stringify({ apps }, null, 2) + '\n')

console.log(`wrote ${agents.length} agents, ${skills.length} skills and ${apps.length} apps to ${out}`)
