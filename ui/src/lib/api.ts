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
