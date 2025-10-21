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

export interface AiSettingsView extends AiSettingsSnapshot {
  warn_ratio: number
  force_ratio: number
  summarizer_model?: string | null
}

export interface AiUpdateSettingsPayload {
  provider_id: string
  model?: string | null
  api_key?: string | null
  base_url?: string | null
  warn_ratio?: number | null
  force_ratio?: number | null
  summarizer_model?: string | null
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

export interface ConversationRecord {
  id: string
  title?: string | null
  provider_id: string
  model_id: string
  ctx_warn: boolean
  ctx_force: boolean
  created_at: number
  updated_at: number
  closed_at?: number | null
  quality_flags?: string | null
  total_tokens: number
}

export interface MessageRecord {
  id: string
  conversation_id: string
  role: string
  body: string
  token_est?: number | null
  created_at: number
  quality_flags?: string | null
}

export interface SummaryRecord {
  id: string
  target_type: string
  target_id: string
  version: number
  body: string
  token_est?: number | null
  model_id?: string | null
  created_at: number
  reused: boolean
}

export interface AppendResult {
  message: MessageRecord
  warn: boolean
  rolled: boolean
  new_conversation?: ConversationRecord | null
  summary?: SummaryRecord | null
  total_tokens: number
}

export interface RolloverOutcome {
  rolled: boolean
  new_conversation?: ConversationRecord | null
  summary?: SummaryRecord | null
}

/** Enumerate all configured AI providers. */
export async function aiListProviders(): Promise<AiProviderInfo[]> {
  return invoke('ai_list_providers')
}

/** Convenience alias for providers list to satisfy the Phase 0 contract. */
export async function aiListModels(): Promise<AiProviderInfo[]> {
  return invoke('ai_list_models')
}

/** Retrieve the active AI configuration snapshot. */
export async function aiGetSettings(): Promise<AiSettingsView> {
  return invoke('ai_get_settings')
}

/** Persist provider/model/credential changes. */
export async function aiUpdateSettings(payload: AiUpdateSettingsPayload): Promise<AiSettingsView> {
  return invoke('ai_update_settings', { input: payload })
}

/** Execute a chat completion against the selected runtime. */
export async function aiChat(payload: AiChatCommand): Promise<AiChatResponse> {
  return invoke('ai_chat', { input: payload })
}

/** Create a new chat conversation row. */
export async function chatCreateConversation(payload: { title?: string | null, provider_id?: string | null, model_id?: string | null }): Promise<ConversationRecord> {
  return invoke('chat_create_conversation', { input: payload })
}

/** List conversations ordered by most recent activity. */
export async function chatListConversations(limit?: number): Promise<ConversationRecord[]> {
  return invoke('chat_list_conversations', { limit })
}

/** Fetch messages for a conversation. */
export async function chatGetMessages(conversation_id: string, limit?: number): Promise<MessageRecord[]> {
  return invoke('chat_get_messages', { input: { conversation_id, limit } })
}

/** Append a message and trigger rollover checks. */
export async function chatAppendAndMaybeRollover(conversation_id: string, content: string, role?: string): Promise<AppendResult> {
  return invoke('chat_append_and_maybe_rollover', { input: { conversation_id, content, role } })
}

/** Force rollover for a conversation. */
export async function aiRolloverChat(conversation_id: string): Promise<RolloverOutcome> {
  return invoke('ai_rollover_chat', { input: { conversation_id } })
}

/** Override the provider/model associated with a conversation. */
export async function aiSetModel(conversation_id: string, provider_id?: string | null, model_id?: string | null): Promise<ConversationRecord> {
  return invoke('ai_set_model', { input: { conversation_id, provider_id, model_id } })
}

/** Request a summary for a particular entity (note, conversation, or day). */
export async function aiSummarize(target_type: string, target_id: string): Promise<SummaryRecord> {
  return invoke('ai_summarize', { input: { target_type, target_id } })
}

/** Retrieve an existing summary by id. */
export async function aiGetSummary(summary_id: string): Promise<SummaryRecord | null> {
  return invoke('ai_get_summary', { input: { summary_id } })
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
