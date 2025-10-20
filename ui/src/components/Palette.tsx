import React, { useEffect, useMemo, useRef, useState } from 'react'

/**
 * Command palette command descriptor consumed by the Palette component.
 * Commands can run synchronously or asynchronously and provide metadata used
 * for fuzzy filtering.
 */
export interface PaletteCommand {
  id: string
  title: string
  description?: string
  keywords?: string[]
  action: () => Promise<void> | void
}

interface PaletteProps {
  open: boolean
  onClose: () => void
  commands: PaletteCommand[]
}

/**
 * Modal command palette inspired by tools like Spotlight and Raycast. The
 * component is deliberately self-contained so it can be embedded anywhere in
 * the UI without additional wiring.
 */
export default function Palette({ open, onClose, commands }: PaletteProps) {
  const [query, setQuery] = useState('')
  const [activeIndex, setActiveIndex] = useState(0)
  const [runningCommand, setRunningCommand] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)
  const inputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    if (!open) return
    setQuery('')
    setActiveIndex(0)
    setError(null)
    const timer = window.setTimeout(() => inputRef.current?.focus(), 50)
    return () => window.clearTimeout(timer)
  }, [open])

  useEffect(() => {
    if (!open) return
    const handleKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault()
        onClose()
      }
    }
    window.addEventListener('keydown', handleKey)
    return () => window.removeEventListener('keydown', handleKey)
  }, [open, onClose])

  const filtered = useMemo(() => {
    if (!query.trim()) return commands
    const terms = query.toLowerCase().split(/\s+/).filter(Boolean)
    return commands.filter((command) => {
      const haystack = [command.title, command.description ?? '', ...(command.keywords ?? [])]
        .join(' ')
        .toLowerCase()
      return terms.every((term) => haystack.includes(term))
    })
  }, [commands, query])

  useEffect(() => {
    if (!error) return
    setError(null)
  }, [query, error])

  useEffect(() => {
    if (activeIndex >= filtered.length) {
      setActiveIndex(Math.max(filtered.length - 1, 0))
    }
  }, [filtered, activeIndex])

  /** Execute the selected command while surfacing any runtime errors. */
  async function execute(command: PaletteCommand) {
    if (runningCommand) return
    try {
      setRunningCommand(command.id)
      await command.action()
      setRunningCommand(null)
      setError(null)
      onClose()
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      setError(message)
      setRunningCommand(null)
    }
  }

  /** Keyboard navigation for the results list. */
  function onKeyDown(event: React.KeyboardEvent<HTMLInputElement>) {
    if (!filtered.length) return
    if (event.key === 'ArrowDown') {
      event.preventDefault()
      setActiveIndex((index) => (index + 1) % filtered.length)
      return
    }
    if (event.key === 'ArrowUp') {
      event.preventDefault()
      setActiveIndex((index) => (index - 1 + filtered.length) % filtered.length)
      return
    }
    if (event.key === 'Enter') {
      event.preventDefault()
      execute(filtered[Math.max(activeIndex, 0)])
    }
  }

  if (!open) return null

  return (
    <div
      onClick={onClose}
      style={{
        position: 'fixed',
        inset: 0,
        background: 'rgba(8, 9, 15, 0.72)',
        backdropFilter: 'blur(6px)',
        display: 'flex',
        alignItems: 'flex-start',
        justifyContent: 'center',
        paddingTop: 120,
        zIndex: 40,
      }}
    >
      <div
        onClick={(event) => event.stopPropagation()}
        style={{
          width: 'min(640px, 90vw)',
          borderRadius: 16,
          background: '#14151f',
          border: '1px solid #262739',
          boxShadow: '0 20px 60px rgba(0,0,0,0.45)',
          overflow: 'hidden',
          display: 'grid',
        }}
      >
        <div style={{ padding: '16px 18px', borderBottom: '1px solid #1f2030' }}>
          <input
            ref={inputRef}
            value={query}
            onChange={(event) => setQuery(event.target.value)}
            onKeyDown={onKeyDown}
            placeholder="Search commands…"
            style={{
              width: '100%',
              padding: '12px 14px',
              borderRadius: 10,
              border: '1px solid #262839',
              background: '#191a27',
              color: '#f5f5f5',
              fontSize: 16,
            }}
          />
          {error && <div style={{ marginTop: 8, color: '#f8d7d7', fontSize: 13 }}>{error}</div>}
        </div>
        <div style={{ maxHeight: 340, overflowY: 'auto' }}>
          {filtered.length === 0 ? (
            <div style={{ padding: '18px 20px', color: '#8d90a8' }}>No commands match your search.</div>
          ) : (
            <ul style={{ listStyle: 'none', margin: 0, padding: 0 }}>
              {filtered.map((command, index) => {
                const active = index === activeIndex
                return (
                  <li key={command.id}>
                    <button
                      type="button"
                      onClick={() => execute(command)}
                      style={{
                        width: '100%',
                        textAlign: 'left',
                        padding: '14px 20px',
                        background: active ? '#1f2f4a' : 'transparent',
                        border: 'none',
                        borderBottom: '1px solid #1f2030',
                        color: '#f5f5f5',
                        cursor: 'pointer',
                        display: 'grid',
                        gap: 4,
                      }}
                    >
                      <div style={{ fontWeight: 600, display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
                        <span>{command.title}</span>
                        {runningCommand === command.id && (
                          <span style={{ fontSize: 12, color: '#8d90a8' }}>Running…</span>
                        )}
                      </div>
                      {command.description && (
                        <div style={{ fontSize: 13, color: '#9ea0b5' }}>{command.description}</div>
                      )}
                    </button>
                  </li>
                )
              })}
            </ul>
          )}
        </div>
      </div>
    </div>
  )
}
