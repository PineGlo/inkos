import React, { useCallback, useEffect, useMemo, useState } from 'react'
import { listLogbookEntries, listTimelineEvents, runDailyDigest, type LogbookEntry, type TimelineEvent } from '../lib/api'

/** Props accepted by the timeline/logbook module. */
interface TimelineProps {
  refreshKey: number
  onNotify?: (message: string, kind?: 'info' | 'error') => void
}

type LoadingState = 'idle' | 'loading' | 'error'

/**
 * Daily timeline and logbook view. Fetches historical data and offers an
 * inline action to regenerate the digest for the selected day.
 */
export default function Timeline({ refreshKey, onNotify }: TimelineProps) {
  const [entries, setEntries] = useState<LogbookEntry[]>([])
  const [selectedDate, setSelectedDate] = useState<string>('')
  const [events, setEvents] = useState<TimelineEvent[]>([])
  const [entriesState, setEntriesState] = useState<LoadingState>('idle')
  const [timelineState, setTimelineState] = useState<LoadingState>('idle')
  const [error, setError] = useState<string | null>(null)
  const [digestBusy, setDigestBusy] = useState(false)

  const fetchEntries = useCallback(
    async (preferredDate?: string) => {
      setEntriesState('loading')
      setError(null)
      try {
        const data = await listLogbookEntries(14)
        setEntries(data)
        if (data.length === 0) {
          setSelectedDate('')
          setEvents([])
          return
        }
        const nextDate = preferredDate
          ? preferredDate
          : selectedDate && data.some((entry) => entry.entry_date === selectedDate)
          ? selectedDate
          : data[0].entry_date
        setSelectedDate(nextDate)
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err)
        setError(message)
        setEntriesState('error')
      } finally {
        setEntriesState('idle')
      }
    },
    [selectedDate]
  )

  const fetchTimeline = useCallback(async (dateKey: string) => {
    if (!dateKey) {
      setEvents([])
      return
    }
    setTimelineState('loading')
    setError(null)
    try {
      const data = await listTimelineEvents(dateKey)
      setEvents(data)
      setTimelineState('idle')
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      setError(message)
      setTimelineState('error')
    }
  }, [])

  useEffect(() => {
    fetchEntries()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshKey])

  useEffect(() => {
    if (selectedDate) {
      fetchTimeline(selectedDate)
    }
  }, [selectedDate, fetchTimeline])

  const selectedEntry = useMemo(() => entries.find((entry) => entry.entry_date === selectedDate), [entries, selectedDate])

  /** Re-run the daily digest for the selected entry and refresh state. */
  async function regenerateDigest() {
    setDigestBusy(true)
    setError(null)
    try {
      const result = await runDailyDigest(selectedDate || undefined)
      const entryDate = result.result?.entry_date ?? selectedDate
      await fetchEntries(entryDate)
      if (onNotify) {
        onNotify(`Daily digest refreshed for ${entryDate}.`, 'info')
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      setError(message)
      if (onNotify) onNotify(message, 'error')
    } finally {
      setDigestBusy(false)
    }
  }

  const timelineContent = useMemo(() => {
    if (timelineState === 'loading') {
      return <div style={{ padding: '12px 16px', color: '#8d90a8' }}>Loading timeline…</div>
    }
    if (timelineState === 'error') {
      return <div style={{ padding: '12px 16px', color: '#f4bebe' }}>Failed to load the timeline for this day.</div>
    }
    if (events.length === 0) {
      return <div style={{ padding: '12px 16px', color: '#8d90a8' }}>No timeline events recorded for this day yet.</div>
    }
    return (
      <ul style={{ listStyle: 'none', margin: 0, padding: 0, display: 'grid', gap: 12 }}>
        {events.map((event) => (
          <li
            key={event.id}
            style={{
              borderRadius: 12,
              border: '1px solid #25263a',
              background: '#161722',
              padding: '12px 16px',
              display: 'grid',
              gap: 6,
            }}
          >
            <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 8 }}>
              <span style={{ fontWeight: 600 }}>{event.title}</span>
              <span style={{ fontSize: 12, color: '#8d90a8' }}>
                {new Date(event.event_time * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
              </span>
            </div>
            {event.detail && <div style={{ color: '#b7b9cd', fontSize: 14 }}>{event.detail}</div>}
            <div style={{ fontSize: 11, textTransform: 'uppercase', letterSpacing: 0.6, color: '#616381' }}>{event.kind}</div>
          </li>
        ))}
      </ul>
    )
  }, [events, timelineState])

  return (
    <div style={{ padding: 24, color: '#f5f5f5', maxWidth: 960 }}>
      <header style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 16, flexWrap: 'wrap' }}>
        <div>
          <h2 style={{ margin: 0 }}>Daily Logbook &amp; Timeline</h2>
          <p style={{ margin: '6px 0 0', color: '#9ea0b5' }}>
            InkOS captures a daily digest summarising your notes, AI usage, and background automation.
          </p>
        </div>
        <button
          type="button"
          onClick={regenerateDigest}
          disabled={digestBusy}
          style={{
            padding: '10px 16px',
            borderRadius: 10,
            border: 'none',
            background: digestBusy ? '#2b2d3c' : '#3c82f6',
            color: '#f5f5f5',
            fontWeight: 600,
            cursor: digestBusy ? 'not-allowed' : 'pointer',
          }}
        >
          {digestBusy ? 'Regenerating…' : 'Refresh daily digest'}
        </button>
      </header>

      {error && (
        <div
          style={{
            marginTop: 16,
            padding: '12px 16px',
            borderRadius: 12,
            background: '#472b2b',
            border: '1px solid #703b3b',
            color: '#f8d7d7',
          }}
        >
          {error}
        </div>
      )}

      <section style={{ display: 'grid', gap: 24, marginTop: 24, gridTemplateColumns: 'minmax(0, 280px) minmax(0, 1fr)' }}>
        <aside
          style={{
            borderRadius: 12,
            border: '1px solid #1f2030',
            background: '#14151f',
            padding: 16,
            maxHeight: 420,
            overflow: 'auto',
          }}
        >
          <h3 style={{ marginTop: 0, fontSize: 16 }}>Logbook entries</h3>
          {entriesState === 'loading' && <div style={{ color: '#8d90a8' }}>Loading entries…</div>}
          {entries.length === 0 && entriesState !== 'loading' && (
            <div style={{ color: '#8d90a8' }}>No logbook entries yet. Try refreshing the daily digest.</div>
          )}
          <ul style={{ listStyle: 'none', margin: 0, padding: 0, display: 'grid', gap: 8 }}>
            {entries.map((entry) => {
              const active = entry.entry_date === selectedDate
              return (
                <li key={entry.id}>
                  <button
                    type="button"
                    onClick={() => setSelectedDate(entry.entry_date)}
                    style={{
                      width: '100%',
                      textAlign: 'left',
                      padding: '10px 12px',
                      borderRadius: 10,
                      border: '1px solid',
                      borderColor: active ? '#3c82f6' : '#24263a',
                      background: active ? '#1f2f4a' : '#191a27',
                      color: active ? '#f5f5f5' : '#c5c7dc',
                      cursor: 'pointer',
                    }}
                  >
                    <div style={{ fontWeight: 600 }}>{new Date(entry.entry_date).toLocaleDateString()}</div>
                    <div style={{ fontSize: 12, color: '#8d90a8' }}>
                      {new Date(entry.created_at * 1000).toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' })}
                    </div>
                  </button>
                </li>
              )
            })}
          </ul>
        </aside>
        <div
          style={{
            borderRadius: 12,
            border: '1px solid #1f2030',
            background: '#14151f',
            padding: 24,
            minHeight: 320,
          }}
        >
          <div style={{ display: 'grid', gap: 12 }}>
            {selectedEntry ? (
              <div>
                <h3 style={{ margin: '0 0 8px' }}>{new Date(selectedEntry.entry_date).toLocaleDateString(undefined, { weekday: 'long', year: 'numeric', month: 'short', day: 'numeric' })}</h3>
                <p style={{ margin: 0, color: '#c5c7dc', lineHeight: 1.6 }}>{selectedEntry.summary}</p>
              </div>
            ) : (
              <div style={{ color: '#8d90a8' }}>Select a logbook entry to review the summary.</div>
            )}
            <div style={{ borderTop: '1px solid #1f2030', margin: '16px 0 0', paddingTop: 16 }}>
              <h4 style={{ margin: '0 0 12px' }}>Timeline</h4>
              {timelineContent}
            </div>
          </div>
        </div>
      </section>
    </div>
  )
}
