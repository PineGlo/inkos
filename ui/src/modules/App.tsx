import React, { useEffect, useState } from 'react'
import { ping, createNote, listNotes } from '../lib/api'

export default function App() {
  const [status, setStatus] = useState('starting...')
  const [title, setTitle] = useState('Hello InkOS (v2)')
  const [notes, setNotes] = useState<Array<{id:string,title:string,created_at:number}>>([])

  useEffect(() => {
    (async () => {
      const r = await ping()
      setStatus('Core online @ ' + new Date(r.ts * 1000).toLocaleString())
      setNotes(await listNotes())
    })()
  }, [])

  async function addNote() {
    const { id } = await createNote({ title, body: 'Created from UI v2' })
    setNotes(await listNotes())
    alert('Note created: ' + id)
  }

  return (
    <div style={{ padding: 16, color: 'white', background: '#0f0f12', height: '100vh', fontFamily: 'Inter, system-ui, sans-serif' }}>
      <h1 style={{ marginTop: 0 }}>InkOS (Tauri v2)</h1>
      <p>{status}</p>
      <div style={{ marginTop: 16 }}>
        <input value={title} onChange={(e) => setTitle(e.target.value)} placeholder="Note title" style={{ padding: 8, width: 300 }} />
        <button onClick={addNote} style={{ marginLeft: 8, padding: '8px 12px' }}>Create Note</button>
      </div>
      <h2 style={{ marginTop: 24 }}>Notes</h2>
      <ul>
        {notes.map(n => (
          <li key={n.id}>{n.title} <small>({new Date(n.created_at*1000).toLocaleString()})</small></li>
        ))}
      </ul>
    </div>
  )
}
