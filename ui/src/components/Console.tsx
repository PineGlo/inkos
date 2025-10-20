import React, { useEffect, useState } from 'react'
import { listAiEvents, type AiRuntimeEvent } from '../lib/api'

/** Props consumed by the AI debugger console modal. */
interface ConsoleProps {
  open: boolean
  onClose: () => void
}

/**
 * Modal surface that renders the AI runtime event log. Fetching happens when
 * the dialog opens so the UI always shows the most recent diagnostics.
 */
export default function Console({ open, onClose }: ConsoleProps) {
  const [events, setEvents] = useState<AiRuntimeEvent[]>([])
  const [loading, setLoading] = useState(false)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => {
    if (!open) return
    loadEvents()
  }, [open])

  useEffect(() => {
    if (!open) return
    const handler = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault()
        onClose()
      }
    }
    window.addEventListener('keydown', handler)
    return () => window.removeEventListener('keydown', handler)
  }, [open, onClose])

  /** Load the latest AI runtime events. */
  async function loadEvents() {
    setLoading(true)
    setError(null)
    try {
      const data = await listAiEvents(40)
      setEvents(data)
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      setError(message)
    } finally {
      setLoading(false)
    }
  }

  if (!open) return null

  return (
    <div
      onClick={onClose}
      style={{
        position: 'fixed',
        inset: 0,
        background: 'rgba(5, 6, 12, 0.78)',
        backdropFilter: 'blur(8px)',
        display: 'flex',
        justifyContent: 'center',
        alignItems: 'flex-start',
        paddingTop: 80,
        zIndex: 50,
      }}
    >
      <div
        onClick={(event) => event.stopPropagation()}
        style={{
          width: 'min(860px, 94vw)',
          maxHeight: '80vh',
          borderRadius: 18,
          background: '#10111a',
          border: '1px solid #25263a',
          boxShadow: '0 24px 80px rgba(0, 0, 0, 0.5)',
          overflow: 'hidden',
          display: 'flex',
          flexDirection: 'column',
        }}
      >
        <header
          style={{
            display: 'flex',
            alignItems: 'center',
            justifyContent: 'space-between',
            padding: '16px 20px',
            borderBottom: '1px solid #1c1d2b',
            color: '#f5f5f5',
          }}
        >
          <div>
            <h3 style={{ margin: 0 }}>AI Debugger Console</h3>
            <p style={{ margin: '4px 0 0', color: '#8d90a8', fontSize: 13 }}>
              Inspect recent AI invocations, usage metrics, and failure signals from the runtime.
            </p>
          </div>
          <div style={{ display: 'flex', gap: 12 }}>
            <button
              type="button"
              onClick={loadEvents}
              disabled={loading}
              style={{
                padding: '8px 14px',
                borderRadius: 10,
                border: '1px solid #2a2c3f',
                background: loading ? '#1b1c27' : '#1f2f4a',
                color: '#f5f5f5',
                cursor: loading ? 'not-allowed' : 'pointer',
                fontWeight: 600,
              }}
            >
              {loading ? 'Refreshing…' : 'Refresh'}
            </button>
            <button
              type="button"
              onClick={onClose}
              style={{
                padding: '8px 14px',
                borderRadius: 10,
                border: '1px solid #2a2c3f',
                background: '#191a27',
                color: '#f5f5f5',
                cursor: 'pointer',
                fontWeight: 600,
              }}
            >
              Close
            </button>
          </div>
        </header>
        {error && (
          <div style={{ padding: '12px 20px', color: '#f8d7d7', background: '#472b2b', borderBottom: '1px solid #703b3b' }}>
            {error}
          </div>
        )}
        <div style={{ flex: 1, overflow: 'auto', padding: '16px 20px', display: 'grid', gap: 12 }}>
          {loading && events.length === 0 && <div style={{ color: '#8d90a8' }}>Loading events…</div>}
          {!loading && events.length === 0 && !error && (
            <div style={{ color: '#8d90a8' }}>No AI runtime events recorded yet.</div>
          )}
          {events.map((event) => {
            const palette = badgeColor(event.level)
            return (
              <article
                key={event.id}
                style={{
                  borderRadius: 12,
                  border: '1px solid #1f2030',
                  background: '#14151f',
                  padding: '14px 16px',
                  display: 'grid',
                  gap: 8,
                }}
              >
                <header style={{ display: 'flex', justifyContent: 'space-between', gap: 12, alignItems: 'center' }}>
                  <div style={{ display: 'flex', gap: 8, alignItems: 'center' }}>
                    <span
                      style={{
                        display: 'inline-flex',
                        alignItems: 'center',
                        padding: '2px 8px',
                        borderRadius: 999,
                        fontSize: 11,
                        letterSpacing: 0.6,
                        textTransform: 'uppercase',
                        background: palette.background,
                        color: palette.color,
                      }}
                    >
                      {event.level}
                    </span>
                    {event.code && <span style={{ fontSize: 12, color: '#8d90a8' }}>{event.code}</span>}
                  </div>
                  <span style={{ fontSize: 12, color: '#8d90a8' }}>
                    {new Date(event.ts * 1000).toLocaleString()}
                  </span>
                </header>
                <div style={{ color: '#f5f5f5', fontWeight: 600 }}>{event.message}</div>
                {event.explain && <div style={{ color: '#b7b9cd', fontSize: 14 }}>{event.explain}</div>}
                {event.data != null && (
                  <pre
                    style={{
                      margin: 0,
                      padding: '12px',
                      borderRadius: 10,
                      background: '#0f1018',
                      color: '#9ea0b5',
                      fontSize: 12,
                      overflow: 'auto',
                    }}
                  >
                    {JSON.stringify(event.data as any, null, 2)}
                  </pre>
                )}
              </article>
            )
          })}
        </div>
      </div>
    </div>
  )
}

/** Presentational helper that maps log levels onto colour tokens. */
function badgeColor(level: string): { background: string; color: string } {
  switch (level) {
    case 'error':
      return { background: 'rgba(169, 46, 62, 0.24)', color: '#f8d7d7' }
    case 'warn':
    case 'warning':
      return { background: 'rgba(219, 174, 64, 0.22)', color: '#f8efc7' }
    default:
      return { background: 'rgba(88, 129, 201, 0.24)', color: '#d5e0ff' }
  }
}
