import { invoke } from '@tauri-apps/api/core'

export async function ping(): Promise<{ok: boolean, ts: number}> {
  return invoke('ping')
}
export async function dbStatus(): Promise<any> {
  return invoke('db_status')
}
export async function createNote(input: { title: string, body?: string }): Promise<{id:string}> {
  return invoke('create_note', { input })
}
export async function listNotes(q?: string): Promise<Array<{id:string,title:string,created_at:number}>> {
  return invoke('list_notes', { input: q ? { q } : undefined })
}

export interface AiProviderInfo {
  id: string
  kind: string
  display_name: string
  description?: string | null
  base_url?: string | null
  default_model?: string | null
  models: string[]
  capability_tags: string[]
  requires_api_key: boolean
  has_credentials: boolean
}

export interface AiSettingsSnapshot {
  active_provider_id?: string | null
  active_model?: string | null
  provider?: AiProviderInfo | null
}

export interface AiUpdateSettingsPayload {
  provider_id: string
  model?: string | null
  api_key?: string | null
  base_url?: string | null
}

export interface AiChatMessage {
  role: string
  content: string
}

export interface AiChatCommand {
  messages: AiChatMessage[]
  temperature?: number
  provider_id?: string
  model?: string
}

export interface AiUsageMetrics {
  prompt_tokens?: number
  completion_tokens?: number
  total_tokens?: number
}

export interface AiChatResponse {
  provider_id: string
  model: string
  content: string
  usage?: AiUsageMetrics
  raw: unknown
}

export async function aiListProviders(): Promise<AiProviderInfo[]> {
  return invoke('ai_list_providers')
}

export async function aiGetSettings(): Promise<AiSettingsSnapshot> {
  return invoke('ai_get_settings')
}

export async function aiUpdateSettings(payload: AiUpdateSettingsPayload): Promise<AiSettingsSnapshot> {
  return invoke('ai_update_settings', { input: payload })
}

export async function aiChat(payload: AiChatCommand): Promise<AiChatResponse> {
  return invoke('ai_chat', { input: payload })
}
