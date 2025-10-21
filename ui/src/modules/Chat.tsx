import React, { useCallback, useEffect, useMemo, useState } from 'react'
import {
  aiChat,
  aiRolloverChat,
  chatAppendAndMaybeRollover,
  chatCreateConversation,
  chatGetMessages,
  chatListConversations,
  type AiChatMessage,
  type AppendResult,
  type ConversationRecord,
  type MessageRecord,
  type SummaryRecord,
} from '../lib/api'

function formatTimestamp(ts: number): string {
  return new Date(ts * 1000).toLocaleString()
}

type ChatProps = {
  onNotify?: (message: string, kind?: 'info' | 'error') => void
}

export default function Chat({ onNotify }: ChatProps) {
  const [conversations, setConversations] = useState<ConversationRecord[]>([])
  const [selectedId, setSelectedId] = useState<string | null>(null)
  const [messages, setMessages] = useState<MessageRecord[]>([])
  const [input, setInput] = useState('')
  const [loadingConversations, setLoadingConversations] = useState(true)
  const [loadingMessages, setLoadingMessages] = useState(false)
  const [sending, setSending] = useState(false)
  const [warnActive, setWarnActive] = useState(false)
  const [rolloverSummary, setRolloverSummary] = useState<SummaryRecord | null>(null)
  const [summaryOpen, setSummaryOpen] = useState(false)
  const [statusMessage, setStatusMessage] = useState<string | null>(null)
  const [error, setError] = useState<string | null>(null)

  const selectedConversation = useMemo(
    () => conversations.find((conversation) => conversation.id === selectedId) ?? null,
    [conversations, selectedId]
  )

  const refreshConversations = useCallback(
    async (preferId?: string | null): Promise<ConversationRecord[]> => {
      setLoadingConversations(true)
      try {
        const list = await chatListConversations(50)
        setConversations(list)
        setSelectedId((prev) => {
          if (preferId) return preferId
          if (prev && list.some((conversation) => conversation.id === prev)) {
            return prev
          }
          return list.length > 0 ? list[0].id : null
        })
        return list
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err)
        setError(message)
        onNotify?.(message, 'error')
        return []
      } finally {
        setLoadingConversations(false)
      }
    },
    [onNotify]
  )

  const loadMessages = useCallback(
    async (conversationId: string, listOverride?: ConversationRecord[]): Promise<MessageRecord[]> => {
      setLoadingMessages(true)
      try {
        const data = await chatGetMessages(conversationId, 200)
        setMessages(data)
        const source = listOverride ?? conversations
        const conversation = source.find((item) => item.id === conversationId)
        setWarnActive(Boolean(conversation?.ctx_warn))
        return data
      } catch (err) {
        const message = err instanceof Error ? err.message : String(err)
        setError(message)
        onNotify?.(message, 'error')
        return []
      } finally {
        setLoadingMessages(false)
      }
    },
    [conversations, onNotify]
  )

  useEffect(() => {
    refreshConversations().catch(() => {})
  }, [refreshConversations])

  useEffect(() => {
    if (!selectedId) {
      setMessages([])
      setWarnActive(false)
      return
    }
    loadMessages(selectedId).catch(() => {})
  }, [selectedId, loadMessages])

  useEffect(() => {
    if (!statusMessage && !error) return
    const timer = window.setTimeout(() => {
      setStatusMessage(null)
      setError(null)
    }, 4000)
    return () => window.clearTimeout(timer)
  }, [statusMessage, error])

  async function ensureConversation(): Promise<{ id: string; record: ConversationRecord | null; list: ConversationRecord[] }> {
    if (selectedConversation) {
      return { id: selectedConversation.id, record: selectedConversation, list: conversations }
    }
    const created = await chatCreateConversation({ title: `Chat ${new Date().toLocaleTimeString()}` })
    const list = await refreshConversations(created.id)
    setRolloverSummary(null)
    setSummaryOpen(false)
    return { id: created.id, record: created, list }
  }

  async function handleSend() {
    if (sending) return
    const content = input.trim()
    if (!content) return
    setSending(true)
    setStatusMessage(null)
    setError(null)
    setRolloverSummary(null)
    setSummaryOpen(false)
    try {
      const ensured = await ensureConversation()
      let conversationId = ensured.id
      let conversationRecord =
        ensured.list.find((item) => item.id === conversationId) ?? ensured.record ?? selectedConversation
      let conversationList = ensured.list.length ? ensured.list : conversations

      let appendResult: AppendResult = await chatAppendAndMaybeRollover(conversationId, content, 'user')

      if (appendResult.rolled && appendResult.new_conversation) {
        const newConversation = appendResult.new_conversation
        setRolloverSummary(appendResult.summary ?? null)
        setStatusMessage('Conversation rolled over to keep the context lean.')
        conversationList = await refreshConversations(newConversation.id)
        conversationRecord = conversationList.find((item) => item.id === newConversation.id) ?? newConversation
        conversationId = newConversation.id
        appendResult = await chatAppendAndMaybeRollover(conversationId, content, 'user')
        if (appendResult.rolled && appendResult.new_conversation) {
          setError('Message triggered multiple consecutive rollovers. Please try a shorter prompt.')
          await refreshConversations(appendResult.new_conversation.id)
          return
        }
      } else {
        conversationList = await refreshConversations(conversationId)
        conversationRecord = conversationList.find((item) => item.id === conversationId) ?? conversationRecord
      }

      setInput('')
      const history = await loadMessages(conversationId, conversationList)

      const providerId = conversationRecord?.provider_id
      const modelId = conversationRecord?.model_id
      if (!providerId || !modelId) {
        throw new Error('Conversation is missing provider/model metadata.')
      }
      const aiMessages: AiChatMessage[] = history.map((message) => ({ role: message.role, content: message.body }))
      const reply = await aiChat({
        provider_id: providerId,
        model: modelId,
        messages: aiMessages,
      })

      let assistantResult = await chatAppendAndMaybeRollover(conversationId, reply.content, 'assistant')
      if (assistantResult.rolled && assistantResult.new_conversation) {
        const newConversation = assistantResult.new_conversation
        setRolloverSummary(assistantResult.summary ?? null)
        setStatusMessage('Assistant response caused a rollover. Showing the new thread.')
        conversationList = await refreshConversations(newConversation.id)
        conversationRecord = newConversation
        conversationId = newConversation.id
      } else {
        setStatusMessage('Assistant replied successfully.')
        conversationList = await refreshConversations(conversationId)
      }

      await loadMessages(conversationId, conversationList)
      setWarnActive(Boolean(conversationList.find((item) => item.id === conversationId)?.ctx_warn || assistantResult.warn))
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      setError(message)
      onNotify?.(message, 'error')
    } finally {
      setSending(false)
    }
  }

  async function handleManualRollover() {
    if (!selectedConversation) return
    setError(null)
    setStatusMessage(null)
    try {
      const outcome = await aiRolloverChat(selectedConversation.id)
      if (outcome.rolled && outcome.new_conversation) {
        setRolloverSummary(outcome.summary ?? null)
        setStatusMessage('Conversation rolled over manually.')
        await refreshConversations(outcome.new_conversation.id)
        await loadMessages(outcome.new_conversation.id)
      } else {
        setStatusMessage('Conversation is already within safe context limits.')
      }
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err)
      setError(message)
      onNotify?.(message, 'error')
    }
  }

  return (
    <div style={{ display: 'flex', minHeight: '100%', background: '#12121a', color: '#f5f5f5' }}>
      <aside
        style={{
          width: 280,
          borderRight: '1px solid #1c1d28',
          padding: 20,
          display: 'flex',
          flexDirection: 'column',
          gap: 16,
          background: '#10111a',
        }}
      >
        <header>
          <h2 style={{ margin: 0 }}>Second-Brain Chat</h2>
          <p style={{ margin: '6px 0 0', color: '#83859b', fontSize: 13 }}>
            Chat with the orchestrator using local-first models. Threads roll automatically when nearing context limits.
          </p>
        </header>
        <button
          type="button"
          onClick={() => {
            chatCreateConversation({ title: `Chat ${new Date().toLocaleTimeString()}` })
              .then((created) => {
                setStatusMessage('New conversation created.')
                setRolloverSummary(null)
                setSummaryOpen(false)
                refreshConversations(created.id).catch(() => {})
              })
              .catch((err) => {
                const message = err instanceof Error ? err.message : String(err)
                setError(message)
                onNotify?.(message, 'error')
              })
          }}
          style={{
            padding: '10px 14px',
            borderRadius: 10,
            border: 'none',
            background: '#3c82f6',
            color: '#f5f5f5',
            fontWeight: 600,
            cursor: 'pointer',
          }}
        >
          Start new conversation
        </button>
        <div style={{ flex: 1, overflow: 'auto', display: 'grid', gap: 12 }}>
          {loadingConversations && <div style={{ color: '#8b8da6' }}>Loading conversations…</div>}
          {!loadingConversations && conversations.length === 0 && (
            <div style={{ color: '#8b8da6' }}>Start a conversation to see it listed here.</div>
          )}
          {conversations.map((conversation) => {
            const active = conversation.id === selectedId
            return (
              <button
                key={conversation.id}
                type="button"
                onClick={() => {
                  setSelectedId(conversation.id)
                  setRolloverSummary(null)
                  setSummaryOpen(false)
                }}
                style={{
                  textAlign: 'left',
                  border: '1px solid ' + (active ? '#3c82f6' : 'rgba(255,255,255,0.08)'),
                  borderRadius: 12,
                  padding: 12,
                  background: active ? '#1c2131' : '#161722',
                  color: '#f5f5f5',
                  cursor: 'pointer',
                }}
              >
                <div style={{ fontWeight: 600, marginBottom: 4 }}>
                  {conversation.title ?? 'Untitled conversation'}
                </div>
                <div style={{ fontSize: 12, color: '#7d7f92' }}>
                  {conversation.model_id} · {conversation.total_tokens} tokens
                </div>
                {conversation.ctx_force && (
                  <div style={{ fontSize: 11, color: '#f9ab7d', marginTop: 6 }}>Rolled over · read-only</div>
                )}
                {!conversation.ctx_force && conversation.ctx_warn && (
                  <div style={{ fontSize: 11, color: '#e6d27d', marginTop: 6 }}>Context nearing limit</div>
                )}
              </button>
            )
          })}
        </div>
      </aside>
      <section style={{ flex: 1, display: 'flex', flexDirection: 'column', position: 'relative' }}>
        <header style={{ padding: '20px 28px 12px', borderBottom: '1px solid #1c1d28' }}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center', flexWrap: 'wrap', gap: 12 }}>
            <div>
              <h2 style={{ margin: 0 }}>{selectedConversation?.title ?? 'Conversation'}</h2>
              <div style={{ color: '#7d7f92', fontSize: 13 }}>
                {selectedConversation ? `${selectedConversation.model_id} · ${selectedConversation.provider_id}` : 'Select a conversation to begin.'}
              </div>
            </div>
            <div style={{ display: 'flex', gap: 12 }}>
              <button
                type="button"
                onClick={handleManualRollover}
                disabled={!selectedConversation || sending}
                style={{
                  padding: '8px 14px',
                  borderRadius: 10,
                  background: '#2f3645',
                  color: '#f5f5f5',
                  border: '1px solid #3d4254',
                  cursor: !selectedConversation || sending ? 'not-allowed' : 'pointer',
                  fontWeight: 600,
                }}
              >
                Force rollover
              </button>
            </div>
          </div>
          {warnActive && !selectedConversation?.ctx_force && (
            <div
              style={{
                marginTop: 12,
                padding: '10px 12px',
                borderRadius: 10,
                background: '#3d3422',
                color: '#f6d48f',
                fontSize: 13,
              }}
            >
              ⚠️ This thread is nearing the model context limit. InkOS will summarise and continue in a fresh conversation soon.
            </div>
          )}
          {selectedConversation?.ctx_force && (
            <div
              style={{
                marginTop: 12,
                padding: '10px 12px',
                borderRadius: 10,
                background: '#2f2538',
                color: '#d9c6f9',
                fontSize: 13,
              }}
            >
              ℹ️ This conversation was rolled over. Continue in the linked successor thread shown in the sidebar.
            </div>
          )}
          {rolloverSummary && (
            <div
              style={{
                marginTop: 12,
                padding: '12px 14px',
                borderRadius: 12,
                background: '#1c2131',
                border: '1px solid #29324a',
              }}
            >
              <button
                type="button"
                onClick={() => setSummaryOpen((value) => !value)}
                style={{
                  background: 'transparent',
                  color: '#9fb4ff',
                  fontWeight: 600,
                  border: 'none',
                  padding: 0,
                  cursor: 'pointer',
                }}
              >
                {summaryOpen ? 'Hide rollover summary' : 'View rollover summary'}
              </button>
              {summaryOpen && (
                <div style={{ marginTop: 8, color: '#c2c6dc', fontSize: 13, whiteSpace: 'pre-wrap' }}>{rolloverSummary.body}</div>
              )}
            </div>
          )}
          {statusMessage && (
            <div style={{ marginTop: 12, color: '#7ce38b', fontSize: 13 }}>{statusMessage}</div>
          )}
          {error && (
            <div style={{ marginTop: 12, color: '#ff9386', fontSize: 13 }}>{error}</div>
          )}
        </header>
        <div style={{ flex: 1, overflow: 'auto', padding: '24px 28px', display: 'grid', gap: 16 }}>
          {loadingMessages && <div style={{ color: '#8b8da6' }}>Loading messages…</div>}
          {!loadingMessages && messages.length === 0 && (
            <div style={{ color: '#8b8da6' }}>Send a message to start the conversation.</div>
          )}
          {messages.map((message) => (
            <div
              key={message.id}
              style={{
                alignSelf: message.role === 'assistant' ? 'flex-start' : message.role === 'system' ? 'stretch' : 'flex-end',
                maxWidth: '70%',
                background:
                  message.role === 'assistant'
                    ? '#1f2435'
                    : message.role === 'system'
                    ? '#1a1c27'
                    : '#28314a',
                color: '#f5f5f5',
                padding: '12px 14px',
                borderRadius: 12,
                border: '1px solid rgba(255,255,255,0.08)',
                boxShadow: '0 8px 20px rgba(0,0,0,0.15)',
              }}
            >
              <div style={{ fontSize: 11, textTransform: 'uppercase', letterSpacing: 0.6, opacity: 0.7, marginBottom: 4 }}>
                {message.role} · {formatTimestamp(message.created_at)}
              </div>
              <div style={{ whiteSpace: 'pre-wrap', fontSize: 14 }}>{message.body}</div>
            </div>
          ))}
        </div>
        <footer
          style={{
            padding: '18px 28px',
            borderTop: '1px solid #1c1d28',
            background: '#13141d',
            display: 'flex',
            gap: 12,
          }}
        >
          <textarea
            value={input}
            onChange={(event) => setInput(event.target.value)}
            placeholder={selectedConversation ? 'Ask something…' : 'Start typing to create a conversation…'}
            rows={3}
            style={{
              flex: 1,
              padding: '12px 14px',
              borderRadius: 12,
              border: '1px solid #2a2c3f',
              background: '#181a24',
              color: '#f5f5f5',
              resize: 'vertical',
            }}
          />
          <button
            type="button"
            onClick={handleSend}
            disabled={sending || !input.trim()}
            style={{
              padding: '12px 20px',
              borderRadius: 12,
              background: sending ? '#2b2d3c' : '#3c82f6',
              color: '#f5f5f5',
              border: 'none',
              cursor: sending || !input.trim() ? 'not-allowed' : 'pointer',
              fontWeight: 600,
              minWidth: 120,
            }}
          >
            {sending ? 'Sending…' : 'Send'}
          </button>
        </footer>
      </section>
    </div>
  )
}
