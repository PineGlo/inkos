import React, { useCallback, useEffect, useMemo, useState } from 'react'
import Chat from './Chat'
import Settings from './Settings'
import Timeline from './Timeline'
import Palette, { type PaletteCommand } from '../components/Palette'
import Console from '../components/Console'
import { ping, createNote, listNotes, runDailyDigest } from '../lib/api'

/**
 * Minimal note object used within the demo sandbox list. The canonical schema
 * lives in the Rust layer but the UI keeps this lightweight for ergonomics.
 */
type Note = { id: string; title: string; created_at: number }

/**
 * Notes sandbox demonstrating synchronous IPC calls (ping + note creation).
 * Notifications bubble up to the root layout so they can be surfaced globally.
 */
function NotesPanel({ onNotify }: { onNotify?: (message: string, kind?: 'info' | 'error') => void }) {
  const [status, setStatus] = useState('Checking core...')
  const [title, setTitle] = useState('Hello InkOS (v2)')
  const [notes, setNotes] = useState<Note[]>([])
  const [busy, setBusy] = useState(false)

  useEffect(() => {
    ;(async () => {
      const response = await ping()
      setStatus('Core online · ' + new Date(response.ts * 1000).toLocaleString())
      setNotes(await listNotes())
    })()
  }, [])

  const sortedNotes = useMemo(() => {
    return [...notes].sort((a, b) => b.created_at - a.created_at)
  }, [notes])

  async function addNote() {
    if (!title.trim()) return
    setBusy(true)
    try {
      const { id } = await createNote({ title: title.trim(), body: 'Created from UI v2' })
      setNotes(await listNotes())
      setTitle('')
      onNotify?.(`Note created: ${id}`, 'info')
    } finally {
      setBusy(false)
    }
  }

  return (
    <div style={{ padding: 32, color: '#f5f5f5', minHeight: '100%', background: '#12121a' }}>
      <h2 style={{ marginTop: 0 }}>Notes Sandbox</h2>
      <p style={{ color: '#9ea0b5' }}>{status}</p>
      <div style={{ display: 'flex', gap: 12, marginTop: 16, flexWrap: 'wrap' }}>
        <input
          value={title}
          onChange={(e) => setTitle(e.target.value)}
          placeholder="Note title"
          style={{
            padding: '10px 12px',
            minWidth: 260,
            borderRadius: 10,
            border: '1px solid #2a2c3f',
            background: '#181a24',
            color: '#f5f5f5',
          }}
        />
        <button
          onClick={addNote}
          disabled={busy}
          style={{
            padding: '10px 16px',
            borderRadius: 10,
            background: busy ? '#2b2d3c' : '#3c82f6',
            color: '#f5f5f5',
            border: 'none',
            cursor: busy ? 'not-allowed' : 'pointer',
            fontWeight: 600,
          }}
        >
          {busy ? 'Creating…' : 'Create Note'}
        </button>
      </div>
      <h3 style={{ marginTop: 32 }}>Recent Notes</h3>
      <ul style={{ padding: 0, listStyle: 'none', margin: 0, display: 'grid', gap: 12, marginTop: 16 }}>
        {sortedNotes.map((note) => (
          <li
            key={note.id}
            style={{
              background: '#181a24',
              padding: '12px 16px',
              borderRadius: 12,
              border: '1px solid #24263a',
            }}
          >
            <div style={{ fontWeight: 600 }}>{note.title}</div>
            <div style={{ color: '#737493', fontSize: 12 }}>
              {new Date(note.created_at * 1000).toLocaleString()}
            </div>
          </li>
        ))}
        {sortedNotes.length === 0 && <li style={{ color: '#737493' }}>Create a note to populate the list.</li>}
      </ul>
    </div>
  )
}

type TabKey = 'notes' | 'chat' | 'timeline' | 'settings'

type Notice = { text: string; kind: 'info' | 'error' }

/**
 * Root InkOS shell component containing the sidebar navigation, command
 * palette, AI console, and routed module panels.
 */
export default function App() {
  const [tab, setTab] = useState<TabKey>('notes')
  const [paletteOpen, setPaletteOpen] = useState(false)
  const [consoleOpen, setConsoleOpen] = useState(false)
  const [notice, setNotice] = useState<Notice | null>(null)
  const [timelineRefreshKey, setTimelineRefreshKey] = useState(0)

  const notify = useCallback((message: string, kind: 'info' | 'error' = 'info') => {
    setNotice({ text: message, kind })
  }, [])

  const dismissNotice = useCallback(() => setNotice(null), [])

  useEffect(() => {
    if (!notice) return
    const timer = window.setTimeout(() => setNotice(null), 4200)
    return () => window.clearTimeout(timer)
  }, [notice])

  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === 'k') {
        event.preventDefault()
        setPaletteOpen(true)
      } else if (event.key === 'Escape') {
        if (paletteOpen) {
          setPaletteOpen(false)
          event.preventDefault()
          return
        }
        if (consoleOpen) {
          setConsoleOpen(false)
          event.preventDefault()
        }
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [paletteOpen, consoleOpen])

  const bumpTimelineRefresh = useCallback(() => {
    setTimelineRefreshKey((value) => value + 1)
  }, [])

  const commands: PaletteCommand[] = useMemo(
    () => [
      {
        id: 'command.daily-digest',
        title: 'Generate daily logbook',
        description: 'Rebuild today\'s logbook summary and refresh the timeline view.',
        keywords: ['logbook', 'timeline', 'digest'],
        action: async () => {
          try {
            const job = await runDailyDigest()
            const entryDate = job.result?.entry_date ?? 'today'
            notify(`Daily digest refreshed for ${entryDate}.`, 'info')
            bumpTimelineRefresh()
            setTab('timeline')
          } catch (error) {
            const message = error instanceof Error ? error.message : String(error)
            notify(message, 'error')
            throw error instanceof Error ? error : new Error(message)
          }
        },
      },
      {
        id: 'command.open-chat',
        title: 'Open chat assistant',
        description: 'Talk with the assistant and manage conversation rollovers.',
        keywords: ['chat', 'assistant', 'rollover'],
        action: () => setTab('chat'),
      },
      {
        id: 'command.open-timeline',
        title: 'Open timeline & logbook',
        description: 'Jump directly to the automated daily chronicle.',
        keywords: ['journal'],
        action: () => setTab('timeline'),
      },
      {
        id: 'command.open-settings',
        title: 'Open AI settings',
        description: 'Configure providers, models, and credentials.',
        keywords: ['ai', 'settings'],
        action: () => setTab('settings'),
      },
      {
        id: 'command.open-debugger',
        title: 'Open AI debugger console',
        description: 'Inspect recent AI runtime calls and diagnostics.',
        keywords: ['debug', 'ai', 'console'],
        action: () => setConsoleOpen(true),
      },
    ],
    [bumpTimelineRefresh, notify]
  )

  return (
    <div
      style={{
        display: 'flex',
        height: '100vh',
        background: '#0f0f12',
        color: '#f5f5f5',
        fontFamily: 'Inter, system-ui, sans-serif',
      }}
    >
      <aside
        style={{
          width: 220,
          borderRight: '1px solid #1c1d28',
          background: '#10111a',
          padding: 24,
          display: 'flex',
          flexDirection: 'column',
          gap: 24,
        }}
      >
        <div>
          <h1 style={{ margin: 0, fontSize: 24 }}>InkOS</h1>
          <p style={{ margin: '6px 0 0', color: '#717389', fontSize: 13 }}>Phase 0 · Kernel</p>
        </div>
        <nav style={{ display: 'grid', gap: 8 }}>
          <NavButton label="Notes" active={tab === 'notes'} onClick={() => setTab('notes')} />
          <NavButton label="Chat" active={tab === 'chat'} onClick={() => setTab('chat')} />
          <NavButton label="Timeline" active={tab === 'timeline'} onClick={() => setTab('timeline')} />
          <NavButton label="AI Settings" active={tab === 'settings'} onClick={() => setTab('settings')} />
        </nav>
        <div style={{ marginTop: 'auto', fontSize: 12, color: '#5d5f76' }}>
          <div>LLM ready</div>
          <div>Switch between cloud & local engines</div>
        </div>
        <div style={{ marginTop: 12, fontSize: 12, color: '#717389' }}>Palette · ⌘K / Ctrl+K</div>
      </aside>
      <main
        style={{
          flex: 1,
          overflow: 'auto',
          background: '#12121a',
          position: 'relative',
          display: 'flex',
          flexDirection: 'column',
        }}
      >
        {notice && (
          <div
            style={{
              position: 'sticky',
              top: 0,
              zIndex: 5,
              display: 'flex',
              alignItems: 'center',
              justifyContent: 'space-between',
              gap: 12,
              padding: '12px 18px',
              background: notice.kind === 'error' ? '#46242a' : '#1d2a44',
              borderBottom: `1px solid ${notice.kind === 'error' ? '#703b3b' : '#2c3a55'}`,
            }}
          >
            <span style={{ fontWeight: 600 }}>{notice.text}</span>
            <button
              type="button"
              onClick={dismissNotice}
              style={{
                padding: '6px 10px',
                borderRadius: 8,
                border: '1px solid rgba(255,255,255,0.12)',
                background: 'transparent',
                color: '#f5f5f5',
                cursor: 'pointer',
                fontSize: 12,
              }}
            >
              Dismiss
            </button>
          </div>
        )}
        <div style={{ flex: 1, overflow: 'auto' }}>
          {tab === 'notes' && <NotesPanel onNotify={notify} />}
          {tab === 'chat' && <Chat onNotify={notify} />}
          {tab === 'timeline' && <Timeline refreshKey={timelineRefreshKey} onNotify={notify} />}
          {tab === 'settings' && <Settings />}
        </div>
      </main>
      <Palette open={paletteOpen} onClose={() => setPaletteOpen(false)} commands={commands} />
      <Console open={consoleOpen} onClose={() => setConsoleOpen(false)} />
    </div>
  )
}

/** Presentational sidebar navigation button. */
function NavButton({ label, active, onClick }: { label: string; active: boolean; onClick: () => void }) {
  return (
    <button
      type="button"
      onClick={onClick}
      style={{
        padding: '10px 14px',
        borderRadius: 10,
        textAlign: 'left',
        background: active ? '#3c82f6' : '#161722',
        color: active ? '#f5f5f5' : '#cbcde0',
        border: 'none',
        cursor: 'pointer',
        fontWeight: 600,
      }}
    >
      {label}
    </button>
  )
}
