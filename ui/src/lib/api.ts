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

export interface LogbookEntry {
  id: string
  entry_date: string
  summary: string
  created_at: number
}

export interface TimelineEvent {
  id: string
  entry_date: string
  event_time: number
  kind: string
  title: string
  detail?: string | null
  created_at: number
}

export interface AiRuntimeEvent {
  id: string
  ts: number
  level: string
  code?: string | null
  message: string
  explain?: string | null
  data?: unknown
}

export interface JobRunResult<T = unknown> {
  job_id: string
  kind: string
  state: string
  result: T
}

export async function listLogbookEntries(limit?: number): Promise<LogbookEntry[]> {
  return invoke('list_logbook_entries', { limit })
}

export async function listTimelineEvents(date?: string): Promise<TimelineEvent[]> {
  return invoke('list_timeline_events', { date })
}

export async function listAiEvents(limit?: number): Promise<AiRuntimeEvent[]> {
  return invoke('list_ai_events', { limit })
}

export async function runDailyDigest(date?: string): Promise<JobRunResult<{ entry_date: string; logbook: LogbookEntry; timeline: TimelineEvent[] }>> {
  return invoke('run_daily_digest', { date })
}
