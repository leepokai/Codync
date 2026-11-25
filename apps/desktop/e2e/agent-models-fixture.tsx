// Mounted inside the running sandbox Electron window by agent-models-login-profile.spec.ts:
// exercises the real BotSettingsPanel, its ModelPicker and FallbacksRow (logins picked in the panel's rail)
// against recorded `agentModels` requests across default / Pro #7 / Ultra #8 login profiles.
import { normalizeBot, type Backend, type BotDraft } from '../src/shared/models'
import { Observable } from '../src/renderer/lib/observable'
import type { AppModel } from '../src/renderer/store/app-model'
import type { BotStore } from '../src/renderer/store/bot-store'
import { AppContext, StoreContext } from '../src/renderer/store/context'
import { BotSettingsPanel } from '../src/renderer/views/bots/BotEditorView'
import type { AgentModels, ModelOption } from '../src/shared/agent-models'
import { mountOverlay } from './overlay'

export interface RecordedCall {
  method: string
  body: unknown
}

const EFFORT: ModelOption = {
  id: 'reasoning_effort',
  name: 'Reasoning effort',
  values: [
    { id: 'low', name: 'Low' },
    { id: 'medium', name: 'Medium' },
    { id: 'high', name: 'High' },
  ],
  default: 'medium',
}

const CATALOGS: Record<string, { id: string; name: string; options?: ModelOption[] }[]> = {
  '': [
    { id: 'gemini-3.8-flash', name: 'Gemini 3.8 Flash', options: [EFFORT] },
  ],
  'pro-7': [
    { id: 'gemini-4-neon', name: 'Gemini 4 Neon' },
    { id: 'gemini-4-argon', name: 'Gemini 4 Argon' },
  ],
  'ultra-8': [
    { id: 'gemini-4-neon', name: 'Gemini 4 Neon' },
    { id: 'gemini-4-carbon', name: 'Gemini 4 Carbon' },
  ],
}

const BACKENDS: Backend[] = [
  {
    id: 'antigravity',
    name: 'Google Antigravity',
    available: true,
    installed: true,
    description: '',
    installHint: '',
    signedIn: true,
    canInstall: false,
    command: '',
    registry: 'antigravity-acp',
    curated: true,
    defaultLoginProfile: { name: 'Default login profile', label: null, email: 'default@example.com' },
    loginProfiles: [
      { id: 'pro-7', kind: 'account', name: 'Pro #7', label: 'Pro #7', email: 'pro7@example.com', signedIn: true, vars: [] },
      { id: 'ultra-8', kind: 'account', name: 'Ultra #8', label: 'Ultra #8', email: 'ultra8@example.com', signedIn: true, vars: [] },
    ],
  },
]

class FakeStore extends Observable {
  readonly computer = { id: 'e2e-computer', name: 'E2E PC' }
  readonly connection = { kind: 'online' as const }
  readonly hostName = 'E2E PC'
  readonly icons = {}
  readonly screen = null
  readonly calls: RecordedCall[] = []
  private readonly modelLists = new Map<string, AgentModels>()
  readonly bots = new Map([
    [
      'bot-1',
      normalizeBot({
        id: 'bot-1',
        name: 'Model Refresh Bot',
        backend: 'antigravity',
        loginProfile: null,
        fallbacks: [
          { backend: 'antigravity', loginProfile: 'pro-7', model: null, modelOptions: {} },
        ],
        selfFallbacks: true,
        threadFallbacks: null,
        maxThreads: null,
        threadWorktrees: null,
        compaction: null,
        manageTeam: null,
      }),
    ],
  ])
  readonly hello = { backends: BACKENDS, home: '' }

  readonly client = {
    call: async (method: string, body: unknown) => {
      this.calls.push({ method, body })
      if (method === 'connectors' || method === 'skills') return { items: [] }
      if (method === 'agentModels') {
        const req = (body ?? {}) as { backend?: string; loginProfile?: string }
        const profile = typeof req.loginProfile === 'string' ? req.loginProfile : ''
        const models = CATALOGS[profile] ?? CATALOGS['']
        return {
          models: models.map((m) => ({ options: [], ...m })),
          currentModelId: models[0]?.id ?? null,
          fastMode: false,
          defaultModelOptions: true,
          modes: [],
        }
      }
      return {}
    },
  }

  modelsFor(backend: string, loginProfile: string | null) {
    return this.modelLists.get(`${backend}/${loginProfile ?? ''}`) ?? null
  }

  keepModels(backend: string, loginProfile: string | null, list: AgentModels) {
    this.modelLists.set(`${backend}/${loginProfile ?? ''}`, list)
    this.changed()
  }

  backendName(id: string) {
    return BACKENDS.find((b) => b.id === id)?.name ?? id
  }

  agentRegistry(id: string) {
    return BACKENDS.find((b) => b.id === id)?.registry ?? id
  }

  async save(_draft: BotDraft) {}
  async switchBack(_botId: string) {}
}

class FakeApp extends Observable {
  readonly local = {}
  readonly host = { dev: true }
}

export function mount() {
  const store = new FakeStore()
  const unmount = mountOverlay(
    'e2e-agent-models',
    <AppContext.Provider value={new FakeApp() as unknown as AppModel}>
      <StoreContext.Provider value={store as unknown as BotStore}>
        <BotSettingsPanel botId="bot-1" />
      </StoreContext.Provider>
    </AppContext.Provider>,
    'z-index:250;flex-direction:column;width:520px;overflow:auto;padding:16px;',
  )
  return {
    unmount,
    calls: () => store.calls.filter((c) => c.method === 'agentModels').map((c) => c.body),
  }
}
