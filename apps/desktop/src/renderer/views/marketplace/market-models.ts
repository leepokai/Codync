import type { AgentAccount, ConnectorFailure } from '@shared/models'
import type { HostClient } from '../../client/host-client'

// Marketplace wire types (host/src/market/): connectors are MCP servers,
// skills are instruction folders; agents come from `Hello.backends`. (kit Market.swift)

export interface InstalledConnector {
  id: string
  name: string
  description: string
  registryName?: string | null
  /** `local` (a command the agent starts), `remote` (a URL) or `composio` (an app connected through Composio). */
  kind: string
  command?: string | null
  url?: string | null
  /** Names of the keys and headers that are set; their values stay on the computer. */
  keys: string[]
  /** Composio apps: the app's logo. */
  logo?: string | null
  /** Remote servers: `none`, `signedOut` (waits for sign-in; bots don't get it yet) or `signedIn`. */
  auth?: string | null
  /** Last MCP connection/tool failure classified by the host (Idea 50). */
  lastFailure?: ConnectorFailure | null
}

export const needsSignIn = (c: InstalledConnector) => c.auth === 'signedOut'

/** Where a connector's sign-in page is and who catches its return: `app` or `host`; `code`: what to type there (GitHub). */
export interface ConnectorSignIn {
  url: string
  callback: string
  code?: string | null
}

export interface ComposioStatus {
  configured: boolean
  /** Where to get an API key. */
  keyUrl: string
}

export interface ComposioApp {
  slug: string
  name: string
  description?: string | null
  logo?: string | null
  /** Composio's status: `active`, `initiated`, `failed`, `expired`… */
  connection?: { id: string; status: string } | null
}

export const appConnected = (a: ComposioApp) => a.connection?.status === 'active'

export interface ComposioField {
  name: string
  label: string
  description?: string | null
  secret: boolean
  required: boolean
}

/** What connecting an app needs: a sign-in page, or fields for a key-based app. */
export interface ComposioConnect {
  /** `redirect` or `needsFields`. */
  status: string
  url?: string | null
  connection?: string | null
  mode?: string | null
  fields?: ComposioField[] | null
}

export interface ComposioConnectionState {
  id: string
  status: string
}

export interface MarketInput {
  name: string
  description?: string | null
  secret: boolean
  required: boolean
  default?: string | null
  placeholder?: string | null
}

export interface InstallOption {
  id: string
  /** npm | pypi | oci | remote */
  kind: string
  label: string
  inputs: MarketInput[]
}

export interface MarketConnector {
  name: string
  title: string
  description?: string | null
  version?: string | null
  website?: string | null
  installed: boolean
  options: InstallOption[]
}

export interface InstalledSkill {
  id: string
  name: string
  description: string
  source: string
  path: string
}

export interface MarketSkill {
  /** A catalog id: `openbot/<id>` for the bundled catalog, a folder name in anthropics/skills otherwise. */
  source: string
  name: string
  description: string
  installed: boolean
  category?: string
  creator?: string
  featured?: boolean
  pack?: string | null
}

/** A marketplace agent: a ready-made bot with its instructions, skills and routines. */
export interface MarketAgent {
  id: string
  name: string
  title: string
  category: string
  creator: string
  version: number
  updated: string
  avatarColor: string
  avatarShape: string
  featured: boolean
  installed: boolean
}

export interface MarketAgentSkill {
  id: string
  name: string
  description: string
  pack: boolean
}

export interface MarketAgentRoutine {
  name: string
  instruction: string
  active: boolean
  schedule: { kind: string; time?: string; weekday?: number; day?: number }
}

export interface MarketAgentDetail extends MarketAgent {
  instructions: string
  skills: MarketAgentSkill[]
  routines: MarketAgentRoutine[]
}

/** A marketplace app: an MCP server with a sign-in, a key, setup steps or nothing. */
export interface MarketApp {
  id: string
  name: string
  tagline: string
  description: string
  category: string
  creator: string
  version: string
  updated: string
  icon?: string | null
  website?: string | null
  featured: boolean
  /** `link`, `open`, `key` or `local`. */
  setup: string
  server: { name: string; transport: string; url?: string | null; command?: string | null; args: string[] }
  fields: { id: string; label: string; env?: string | null; header?: string | null; url: boolean; placeholder?: string | null; hint?: string | null; required: boolean }[]
  steps: string[]
  docsUrl?: string | null
  docsLabel?: string | null
  prompts: { id: string; text: string }[]
  installed: boolean
}

/** The marketplace categories, in the order the tabs show them. */
export const CATEGORIES = ['coding', 'design', 'documents', 'research', 'data-analytics', 'productivity', 'automation', 'other'] as const

export const CATEGORY_LABELS: Record<string, string> = {
  coding: 'Coding',
  design: 'Design',
  documents: 'Documents',
  research: 'Research',
  'data-analytics': 'Data & analytics',
  productivity: 'Productivity',
  automation: 'Automation',
  other: 'Other',
}

export interface ConnectionRequest {
  toolkit?: string | null
  kind: string
  status: string
  title: string
  botId: string
  registryName?: string | null
  connectorId?: string | null
  field?: string | null
  location?: string | null
  /** Login requests: the website's domain or the app's name. */
  site?: string | null
  /** Agent sign-in requests: the agent (backend id) to sign in to. */
  backend?: string | null
  /** App requests: the app's logo, when Composio lists it. */
  logo?: string | null
}

/** A saved website or app login; the password stays on the computer. */
export interface SavedLogin {
  id: string
  site: string
  username: string
}

/** A secret a bot keeps for API calls or commands; the value never leaves the host. */
export interface BotSecret {
  id: string
  botId: string
  botName: string | null
  name: string
  /** Where it goes, worded by the host (`https://api.example.com · bearer token`). */
  destination: string
}

/** The first host that keeps bot secrets. */
export const BOT_SECRETS_HOST = '2.20.0'

export interface CredentialStatus {
  provider: string
  onePasswordConnected: boolean
}

// Agent setup (kit Models.swift)

export type AuthMethodKind = 'terminal' | 'agent' | 'envVar'

export interface AuthVar {
  name: string
  label: string
  secret: boolean
  optional: boolean
}

export interface AuthMethod {
  id: string
  name: string
  description?: string | null
  kind?: AuthMethodKind | null
  vars?: AuthVar[] | null
  link?: string | null
}

/** How an agent can be signed in, from asking the agent itself. */
export interface AgentAuth {
  signedIn?: boolean | null
  /** The agent's own words when it wants signing in (can hold a pairing code). */
  detail?: string | null
  methods: AuthMethod[]
  /** Keys saved on the computer for this agent (names only). */
  savedEnv?: string[] | null
  /** Codync's own sign-in command is available. */
  login?: boolean | null
}

/** What a setup terminal on the computer runs. */
export type SetupStep = 'install' | 'node' | 'login'

const s = (n: number) => n * 1000

// Host calls, same methods and timeouts as kit's HostClient extensions.

export const market = {
  connectors: async (c: HostClient) => (await c.call<{ items: InstalledConnector[] }>('connectors')).items,
  /** A page of the MCP Registry (featured first), or of a search (can take ~30 s). */
  marketConnectors: async (c: HostClient, search: string, cursor: string | null = null) => {
    const res = await c.call<{ items: MarketConnector[]; nextCursor?: string | null }>('marketConnectors', { search, cursor: cursor ?? '' }, s(60))
    return { items: res.items, next: res.nextCursor || null }
  },
  installConnector: async (c: HostClient, registryName: string, option: string, inputs: Record<string, string>) =>
    (await c.call<{ connector: InstalledConnector }>('installConnector', { registryName, option, inputs }, s(45))).connector,
  /** A connector you describe yourself: a command line with env, or an https URL with headers. */
  addConnector: async (c: HostClient, body: { name: string; command: string | null; args?: string[]; url: string | null; env: Record<string, string>; headers: Record<string, string>; catalog?: string; description?: string }) =>
    (await c.call<{ connector: InstalledConnector }>('installConnector', body, s(45))).connector,
  /** Adds every server in a pasted MCP config (`mcpServers` / `servers` JSON, or one server's entry). */
  importConnectors: async (c: HostClient, config: string) => (await c.call<{ items: InstalledConnector[] }>('importConnectors', { config }, s(60))).items,
  // `showsCode`: this app shows a device sign-in's code, so the host may answer with one.
  connectorSignIn: (c: HostClient, id: string) => c.call<ConnectorSignIn>('connectorSignIn', { id, showsCode: true }, s(60)),
  finishConnectorSignIn: async (c: HostClient, state: string, code: string | null, error: string | null) =>
    (await c.call<{ connector: InstalledConnector }>('connectorSignInFinish', { state, code, error }, s(60))).connector,
  removeConnector: (c: HostClient, id: string) => c.call('removeConnector', { id }),
  composioStatus: (c: HostClient) => c.call<ComposioStatus>('composioStatus'),
  /** Checks the key with Composio before keeping it; an empty key forgets Composio. */
  setComposioKey: (c: HostClient, key: string) => c.call<ComposioStatus>('setComposioKey', { key }, s(60)),
  composioApps: (c: HostClient, search: string, cursor = '') =>
    c.call<{ items: ComposioApp[]; nextCursor: string | null }>('composioToolkits', { search, cursor }, s(60)),
  composioConnect: (c: HostClient, app: string) => c.call<ComposioConnect>('composioConnect', { toolkit: app }, s(60)),
  composioConnectFields: (c: HostClient, app: string, mode: string, fields: Record<string, string>) =>
    c.call<ComposioConnectionState>('composioConnectFields', { toolkit: app, mode, fields }, s(60)),
  composioConnection: (c: HostClient, id: string) => c.call<ComposioConnectionState>('composioConnection', { id }, s(30)),
  skills: async (c: HostClient) => (await c.call<{ items: InstalledSkill[] }>('skills')).items,
  marketSkills: async (c: HostClient) => (await c.call<{ items: MarketSkill[] }>('marketSkills', {}, s(60))).items,
  /** Marketplace agents: the ones `search` and `category` keep, with whether this computer has each. */
  marketAgents: async (c: HostClient, search = '', category = '') =>
    (await c.call<{ items: MarketAgent[] }>('marketAgents', { search, category })).items,
  marketAgent: (c: HostClient, id: string) => c.call<MarketAgentDetail>('marketAgent', { id }),
  /** Adds the agent's bot with its look, instructions, skills and routines. */
  installAgent: async (c: HostClient, id: string) => (await c.call<{ bot: { id: string } }>('installAgent', { id }, s(120))).bot,
  marketApps: async (c: HostClient, search = '') => (await c.call<{ items: MarketApp[] }>('marketApps', { search })).items,
  installSkill: (c: HostClient, source: string) => c.call('installSkill', { source }, s(60)),
  addSkill: (c: HostClient, name: string, description: string, instructions: string) => c.call('installSkill', { name, description, instructions }),
  removeSkill: (c: HostClient, id: string) => c.call('removeSkill', { id }),
  /** A skill's `SKILL.md` without its front matter: an installed one by `id`, a catalog one by `source`. */
  skillDetails: async (c: HostClient, which: { id: string } | { source: string }) => (await c.call<{ markdown: string }>('skillDetails', which, s(60))).markdown,
  connectorInfo: (c: HostClient, registryName: string) => c.call<MarketConnector>('connectorInfo', { registryName }, s(60)),
  verifyConnector: (c: HostClient, id: string) => c.call<{ status: string }>('connectorVerify', { id }, s(120)),
  finishConnectionRequest: (c: HostClient, entryId: string, opts: { connectorId?: string; value?: string; username?: string } = {}) =>
    c.call('connectorRequestFinish', { entryId, connectorId: opts.connectorId ?? null, value: opts.value ?? null, username: opts.username ?? null }, s(120)),
  logins: async (c: HostClient) => (await c.call<{ items: SavedLogin[] }>('credentialLogins', {}, s(60))).items,
  saveLogin: (c: HostClient, site: string, username: string, password: string) => c.call('credentialSaveLogin', { site, username, password }, s(60)),
  removeLogin: (c: HostClient, id: string) => c.call('credentialRemoveLogin', { id }, s(60)),
  botSecrets: async (c: HostClient) => (await c.call<{ items: BotSecret[] }>('credentialBotSecrets', {}, s(60))).items,
  removeBotSecret: (c: HostClient, id: string) => c.call('credentialRemoveBotSecret', { id }, s(60)),
  credentialStatus: (c: HostClient) => c.call<CredentialStatus>('credentialStatus', {}, s(60)),
  setOnePasswordToken: (c: HostClient, token: string) => c.call<CredentialStatus>('credentialSetOnePassword', { token }, s(60)),
  updateConnectorCredentials: (c: HostClient, id: string, fields: Record<string, string>) => c.call('credentialUpdateConnector', { id, fields }, s(120)),

  // Agent setup terminals
  /** The first sign-in through a registry build can download it. */
  agentSetup: async (c: HostClient, backend: string, step: SetupStep, method: string | null, cols: number, rows: number) =>
    (await c.call<{ term: string }>('agentSetup', { backend, step, method, cols, rows }, s(300))).term,
  agentAuth: (c: HostClient, backend: string) => c.call<AgentAuth>('agentAuth', { backend }, s(300)),
  agentAuthenticate: (c: HostClient, backend: string, method: string) => c.call<AgentAuth>('agentAuthenticate', { backend, method }, s(660)),
  setAgentEnv: (c: HostClient, backend: string, vars: Record<string, string>) => c.call<AgentAuth>('setAgentEnv', { backend, vars }, s(300)),
  listAccounts: async (c: HostClient, backend?: string) =>
    (await c.call<{ accounts: AgentAccount[] }>('listAccounts', { backend: backend ?? null }, s(30))).accounts,
  accountAuthenticate: (c: HostClient, backend: string, name?: string) =>
    c.call<{ account: AgentAccount }>('accountAuthenticate', { backend, name: name ?? null }, s(660)),
  deleteAccount: (c: HostClient, accountId: string) =>
    c.call('deleteAccount', { accountId }, s(30)),
  termInput: (c: HostClient, term: string, data: string) => c.call('termInput', { term, data }),
  termResize: (c: HostClient, term: string, cols: number, rows: number) => c.call('termResize', { term, cols, rows }),
  termClose: (c: HostClient, term: string) => c.call('termClose', { term }),
}

/** Keep fetched catalog metadata in memory. The Extensions tabs scroll through it. */
export interface MarketplacePage<T> {
  items: T[]
}

export const emptyPage = <T>(): MarketplacePage<T> => ({ items: [] })

export function appendPage<T>(page: MarketplacePage<T>, more: T[], id: (t: T) => string): MarketplacePage<T> {
  const known = new Set(page.items.map(id))
  return { ...page, items: [...page.items, ...more.filter((t) => !known.has(id(t)) && (known.add(id(t)), true))] }
}

export const replacePage = <T>(items: T[], id: (t: T) => string) => appendPage(emptyPage<T>(), items, id)

export type ExtensionSection = 'apps' | 'agents' | 'clis' | 'skills'

/** Each section with the line saying where its items come from. */
export const EXTENSION_SECTIONS: { id: ExtensionSection; label: string; source: string }[] = [
  { id: 'apps', label: 'Apps', source: 'Apps connect a service to {name} with its own sign-in or key. Custom connectors add any MCP server.' },
  { id: 'agents', label: 'Agents', source: 'Ready-made agents come with their own instructions, skills and routines. Add one to {name}.' },
  { id: 'clis', label: 'CLIs', source: 'The coding agents (ACPs) that run bots on {name}. Install one and sign in.' },
  { id: 'skills', label: 'Skills', source: 'Skills install on {name} and every bot can use them.' },
]

export const errorText = (e: unknown) => (e instanceof Error ? e.message : String(e))
export const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms))
