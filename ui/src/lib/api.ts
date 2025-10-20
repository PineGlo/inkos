import { invoke } from '@tauri-apps/api/core'

/**
 * Thin wrapper around Tauri's `invoke` helper so TypeScript callers can work
 * with strongly typed promises. Each function maps directly to a Rust IPC
 * command defined under `core/src/api/v1.rs`.
 */

/** Ping the Rust core to confirm the IPC channel is responsive. */
export async function ping(): Promise<{ ok: boolean, ts: number }> {
  return invoke('ping')
}

/** Retrieve a list of tables present in the SQLite database. */
export async function dbStatus(): Promise<any> {
  return invoke('db_status')
}

/** Create a note record with the supplied title/body. */
export async function createNote(input: { title: string, body?: string }): Promise<{ id: string }> {
  return invoke('create_note', { input })
}

/** Fetch notes optionally filtered by a full-text search query. */
export async function listNotes(q?: string): Promise<Array<{ id: string, title: string, created_at: number }>> {
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

/** Enumerate all configured AI providers. */
export async function aiListProviders(): Promise<AiProviderInfo[]> {
  return invoke('ai_list_providers')
}

/** Retrieve the active AI configuration snapshot. */
export async function aiGetSettings(): Promise<AiSettingsSnapshot> {
  return invoke('ai_get_settings')
}

/** Persist provider/model/credential changes. */
export async function aiUpdateSettings(payload: AiUpdateSettingsPayload): Promise<AiSettingsSnapshot> {
  return invoke('ai_update_settings', { input: payload })
}

/** Execute a chat completion against the selected runtime. */
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

/** Return logbook entries up to the provided limit. */
export async function listLogbookEntries(limit?: number): Promise<LogbookEntry[]> {
  return invoke('list_logbook_entries', { limit })
}

/** Fetch timeline events for a given ISO date string. */
export async function listTimelineEvents(date?: string): Promise<TimelineEvent[]> {
  return invoke('list_timeline_events', { date })
}

/** Load recent AI runtime events for the debugger console. */
export async function listAiEvents(limit?: number): Promise<AiRuntimeEvent[]> {
  return invoke('list_ai_events', { limit })
}

/** Trigger the daily digest job and receive the resulting payload. */
export async function runDailyDigest(date?: string): Promise<JobRunResult<{ entry_date: string; logbook: LogbookEntry; timeline: TimelineEvent[] }>> {
  return invoke('run_daily_digest', { date })
}
