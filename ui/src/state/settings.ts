import { create } from 'zustand'
import type { AiChatResponse, AiProviderInfo, AiSettingsSnapshot, AiUpdateSettingsPayload } from '../lib/api'
import { aiChat, aiGetSettings, aiListProviders, aiUpdateSettings } from '../lib/api'

/** Normalised provider shape tailored for the UI. */
export type UiProvider = {
  id: string
  kind: string
  displayName: string
  description?: string | null
  baseUrl?: string | null
  defaultModel?: string | null
  models: string[]
  capabilityTags: string[]
  requiresApiKey: boolean
  hasCredentials: boolean
}

/** Complete settings store contract exposed via Zustand. */
type SettingsState = {
  providers: UiProvider[]
  loading: boolean
  activeProviderId?: string
  activeModel?: string
  draftProviderId?: string
  draftModel?: string
  draftApiKey: string
  draftBaseUrl: string
  statusMessage?: string
  error?: string
  saving: boolean
  testing: boolean
  lastResponse?: AiChatResponse
  clearSecret: boolean
  loadProviders: () => Promise<void>
  loadCurrentSettings: () => Promise<void>
  selectProvider: (id: string) => void
  setDraftModel: (model: string) => void
  setDraftApiKey: (value: string) => void
  setDraftBaseUrl: (value: string) => void
  markClearSecret: () => void
  saveSettings: () => Promise<void>
  testProvider: () => Promise<void>
  clearStatus: () => void
}

/** Convert API provider records into UI friendly structures. */
function mapProvider(info: AiProviderInfo): UiProvider {
  return {
    id: info.id,
    kind: info.kind,
    displayName: info.display_name,
    description: info.description ?? undefined,
    baseUrl: info.base_url ?? undefined,
    defaultModel: info.default_model ?? undefined,
    models: info.models,
    capabilityTags: info.capability_tags ?? [],
    requiresApiKey: info.requires_api_key,
    hasCredentials: info.has_credentials,
  }
}

/** Choose a sensible default model for the given provider. */
function deriveModel(provider?: UiProvider): string | undefined {
  if (!provider) return undefined
  return provider.defaultModel ?? provider.models[0]
}

/**
 * Zustand store coordinating API calls, optimistic updates, and transient UI
 * state for the AI settings panel.
 */
export const useSettingsStore = create<SettingsState>((set, get) => ({
  providers: [],
  loading: false,
  activeProviderId: undefined,
  activeModel: undefined,
  draftProviderId: undefined,
  draftModel: undefined,
  draftApiKey: '',
  draftBaseUrl: '',
  statusMessage: undefined,
  error: undefined,
  saving: false,
  testing: false,
  lastResponse: undefined,
  clearSecret: false,
  async loadProviders() {
    set({ loading: true, error: undefined })
    try {
      const data = await aiListProviders()
      set((state) => {
        const mapped = data.map(mapProvider)
        let draftProviderId = state.draftProviderId
        if (!draftProviderId && mapped.length > 0) {
          draftProviderId = mapped[0].id
        }
        return {
          providers: mapped,
          loading: false,
          draftProviderId,
          draftModel: state.draftModel ?? deriveModel(mapped.find(p => p.id === draftProviderId)),
        }
      })
    } catch (error) {
      set({ loading: false, error: error instanceof Error ? error.message : String(error) })
    }
  },
  async loadCurrentSettings() {
    try {
      const snapshot: AiSettingsSnapshot = await aiGetSettings()
      set((state) => {
        const provider = snapshot.provider ? mapProvider(snapshot.provider) : undefined
        const providers = state.providers.length ? state.providers : provider ? [provider] : []
        const providerId = snapshot.active_provider_id ?? provider?.id ?? state.draftProviderId
        const resolvedProvider = providers.find(p => p.id === providerId) ?? provider
        return {
          providers,
          activeProviderId: snapshot.active_provider_id ?? undefined,
          activeModel: snapshot.active_model ?? undefined,
          draftProviderId: providerId ?? state.draftProviderId,
          draftModel: snapshot.active_model ?? deriveModel(resolvedProvider) ?? state.draftModel,
          draftBaseUrl: resolvedProvider?.baseUrl ?? state.draftBaseUrl,
          draftApiKey: '',
          clearSecret: false,
        }
      })
    } catch (error) {
      set({ error: error instanceof Error ? error.message : String(error) })
    }
  },
  selectProvider(id) {
    const provider = get().providers.find((p) => p.id === id)
    set({
      draftProviderId: id,
      draftModel: deriveModel(provider),
      draftBaseUrl: provider?.baseUrl ?? '',
      draftApiKey: '',
      clearSecret: false,
      error: undefined,
      statusMessage: undefined,
    })
  },
  setDraftModel(model) {
    set({ draftModel: model })
  },
  setDraftApiKey(value) {
    set({ draftApiKey: value, clearSecret: false })
  },
  setDraftBaseUrl(value) {
    set({ draftBaseUrl: value })
  },
  markClearSecret() {
    set({ clearSecret: true, draftApiKey: '' })
  },
  async saveSettings() {
    const state = get()
    const provider = state.providers.find((p) => p.id === state.draftProviderId)
    if (!provider) {
      set({ error: 'Select a provider before saving.' })
      return
    }
    const trimmedKey = state.draftApiKey.trim()
    if (provider.requiresApiKey && !trimmedKey && !provider.hasCredentials) {
      set({ error: 'This provider requires an API key.' })
      return
    }
    set({ saving: true, error: undefined, statusMessage: undefined })
    try {
      const payload: AiUpdateSettingsPayload = {
        provider_id: provider.id,
        model: state.draftModel ?? null,
        base_url: state.draftBaseUrl ? state.draftBaseUrl : null,
      }
      if (trimmedKey) {
        payload.api_key = trimmedKey
      } else if (state.clearSecret) {
        payload.api_key = ''
      }
      const snapshot = await aiUpdateSettings(payload)
      set((prev) => {
        const updatedProviderInfo = snapshot.provider ? mapProvider(snapshot.provider) : undefined
        const updatedProviders = prev.providers.some((p) => p.id === provider.id)
          ? prev.providers.map((p) =>
              p.id === provider.id
                ? {
                    ...p,
                    baseUrl: updatedProviderInfo?.baseUrl ?? state.draftBaseUrl ?? p.baseUrl,
                    hasCredentials: updatedProviderInfo?.hasCredentials ?? p.hasCredentials,
                  }
                : p
            )
          : [...prev.providers, updatedProviderInfo ?? provider]
        return {
          providers: updatedProviders,
          activeProviderId: snapshot.active_provider_id ?? provider.id,
          activeModel: snapshot.active_model ?? state.draftModel,
          draftApiKey: '',
          clearSecret: false,
          statusMessage: 'Settings saved successfully.',
          saving: false,
        }
      })
    } catch (error) {
      set({
        saving: false,
        error: error instanceof Error ? error.message : String(error),
      })
    }
  },
  async testProvider() {
    const state = get()
    const providerId = state.draftProviderId || state.activeProviderId
    const model = state.draftModel || state.activeModel
    if (!providerId || !model) {
      set({ error: 'Select a provider and model before testing.' })
      return
    }
    set({ testing: true, error: undefined, statusMessage: undefined })
    try {
      const response = await aiChat({
        provider_id: providerId,
        model,
        temperature: 0,
        messages: [
          { role: 'system', content: 'You are assisting the InkOS settings panel.' },
          { role: 'user', content: 'Respond with a short confirmation that the connection works.' },
        ],
      })
      set({
        statusMessage: 'Connection successful.',
        testing: false,
        lastResponse: response,
      })
    } catch (error) {
      set({
        testing: false,
        error: error instanceof Error ? error.message : String(error),
      })
    }
  },
  clearStatus() {
    set({ statusMessage: undefined, error: undefined })
  },
}))
