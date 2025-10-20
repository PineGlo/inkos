import React, { useEffect, useMemo, useState } from 'react'
import { useSettingsStore } from '../state/settings'

function Badge({ label }: { label: string }) {
  return (
    <span
      style={{
        display: 'inline-flex',
        alignItems: 'center',
        padding: '2px 8px',
        borderRadius: 999,
        background: 'rgba(255,255,255,0.08)',
        fontSize: 12,
        marginRight: 8,
      }}
    >
      {label}
    </span>
  )
}

export default function Settings() {
  const {
    providers,
    loading,
    activeProviderId,
    activeModel,
    draftProviderId,
    draftModel,
    draftApiKey,
    draftBaseUrl,
    statusMessage,
    error,
    saving,
    testing,
    lastResponse,
    clearSecret,
    loadProviders,
    loadCurrentSettings,
    selectProvider,
    setDraftModel,
    setDraftApiKey,
    setDraftBaseUrl,
    markClearSecret,
    saveSettings,
    testProvider,
    clearStatus,
  } = useSettingsStore()

  const [showKey, setShowKey] = useState(false)

  useEffect(() => {
    loadProviders()
    loadCurrentSettings()
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [])

  useEffect(() => {
    if (!statusMessage && !error) return
    const timer = window.setTimeout(() => clearStatus(), 4000)
    return () => window.clearTimeout(timer)
  }, [statusMessage, error, clearStatus])

  const selectedProvider = useMemo(() => {
    return providers.find((p) => p.id === draftProviderId) ?? providers.find((p) => p.id === activeProviderId) ?? providers[0]
  }, [providers, draftProviderId, activeProviderId])

  const capabilityBadges = useMemo(() => {
    if (!selectedProvider) return null
    return selectedProvider.capabilityTags.map((cap) => <Badge key={cap} label={cap} />)
  }, [selectedProvider])

  const actionDisabled = saving || testing

  return (
    <div style={{ padding: 24, color: '#f5f5f5', maxWidth: 860 }}>
      <h2 style={{ marginTop: 0 }}>AI Providers & Models</h2>
      <p style={{ maxWidth: 620, color: '#b7b7c9' }}>
        Choose which large language model powers InkOS. Configure premium cloud APIs like GPT-4o or Claude, or point to local
        runtimes such as Ollama and LM Studio for private inference.
      </p>

      <div
        style={{
          display: 'grid',
          gap: 24,
          gridTemplateColumns: 'minmax(0, 1fr)',
        }}
      >
        <section style={{ background: 'rgba(255,255,255,0.04)', borderRadius: 16, padding: 20 }}>
          <header style={{ marginBottom: 16 }}>
            <h3 style={{ margin: 0 }}>Provider</h3>
            <p style={{ margin: '4px 0 0', color: '#9ea0b5' }}>
              Pick a default model engine. Saved credentials stay encrypted in your workspace database.
            </p>
          </header>
          <div style={{ display: 'grid', gap: 16 }}>
            <label style={{ display: 'grid', gap: 8 }}>
              <span style={{ fontSize: 14, color: '#d2d3de' }}>Provider</span>
              <select
                value={draftProviderId ?? ''}
                onChange={(event) => selectProvider(event.target.value)}
                style={{ padding: '10px 12px', borderRadius: 10, background: '#15161d', color: '#f5f5f5', border: '1px solid #262839' }}
                disabled={loading}
              >
                <option value="" disabled>
                  {loading ? 'Loading providers…' : 'Select a provider'}
                </option>
                {providers.map((provider) => (
                  <option key={provider.id} value={provider.id}>
                    {provider.displayName} {provider.kind === 'local' ? '(Local)' : ''}
                  </option>
                ))}
              </select>
            </label>

            {selectedProvider && (
              <div style={{ color: '#b7b7c9', fontSize: 14 }}>
                <p style={{ margin: '4px 0 12px' }}>{selectedProvider.description}</p>
                <div>{capabilityBadges}</div>
              </div>
            )}

            <label style={{ display: 'grid', gap: 8 }}>
              <span style={{ fontSize: 14, color: '#d2d3de' }}>Model</span>
              <select
                value={draftModel ?? ''}
                onChange={(event) => setDraftModel(event.target.value)}
                style={{ padding: '10px 12px', borderRadius: 10, background: '#15161d', color: '#f5f5f5', border: '1px solid #262839' }}
                disabled={!selectedProvider}
              >
                <option value="" disabled>
                  {selectedProvider ? 'Select a model' : 'Choose a provider first'}
                </option>
                {selectedProvider?.models.map((model) => (
                  <option key={model} value={model}>
                    {model}
                  </option>
                ))}
              </select>
            </label>

            {selectedProvider && selectedProvider.requiresApiKey && (
              <label style={{ display: 'grid', gap: 8 }}>
                <span style={{ fontSize: 14, color: '#d2d3de' }}>API Key</span>
                <div style={{ display: 'flex', gap: 8 }}>
                  <input
                    type={showKey ? 'text' : 'password'}
                    value={draftApiKey}
                    onChange={(event) => setDraftApiKey(event.target.value)}
                    placeholder={selectedProvider.hasCredentials && !clearSecret ? '•••••••••• (stored)' : 'sk-...'}
                    style={{
                      flex: 1,
                      padding: '10px 12px',
                      borderRadius: 10,
                      background: '#15161d',
                      color: '#f5f5f5',
                      border: '1px solid #262839',
                    }}
                  />
                  <button
                    type="button"
                    onClick={() => setShowKey((prev) => !prev)}
                    style={{
                      padding: '10px 14px',
                      borderRadius: 10,
                      background: '#262839',
                      color: '#f5f5f5',
                      border: '1px solid #34354a',
                      cursor: 'pointer',
                    }}
                  >
                    {showKey ? 'Hide' : 'Show'}
                  </button>
                  {selectedProvider.hasCredentials && (
                    <button
                      type="button"
                      onClick={markClearSecret}
                      style={{
                        padding: '10px 14px',
                        borderRadius: 10,
                        background: clearSecret ? '#522c2c' : '#34354a',
                        color: '#f5f5f5',
                        border: '1px solid #4a2a2a',
                        cursor: 'pointer',
                      }}
                    >
                      {clearSecret ? 'Will clear' : 'Clear stored key'}
                    </button>
                  )}
                </div>
              </label>
            )}

            {selectedProvider && selectedProvider.kind === 'local' && (
              <label style={{ display: 'grid', gap: 8 }}>
                <span style={{ fontSize: 14, color: '#d2d3de' }}>Endpoint URL</span>
                <input
                  type="text"
                  value={draftBaseUrl}
                  onChange={(event) => setDraftBaseUrl(event.target.value)}
                  placeholder="http://127.0.0.1:11434"
                  style={{
                    padding: '10px 12px',
                    borderRadius: 10,
                    background: '#15161d',
                    color: '#f5f5f5',
                    border: '1px solid #262839',
                  }}
                />
              </label>
            )}
          </div>
          <div style={{ display: 'flex', gap: 12, marginTop: 24, flexWrap: 'wrap' }}>
            <button
              type="button"
              onClick={saveSettings}
              disabled={actionDisabled}
              style={{
                padding: '10px 18px',
                borderRadius: 10,
                background: actionDisabled ? '#2a2b38' : '#3c82f6',
                color: '#f5f5f5',
                border: 'none',
                cursor: actionDisabled ? 'not-allowed' : 'pointer',
                fontWeight: 600,
              }}
            >
              {saving ? 'Saving…' : 'Save settings'}
            </button>
            <button
              type="button"
              onClick={testProvider}
              disabled={actionDisabled}
              style={{
                padding: '10px 18px',
                borderRadius: 10,
                background: actionDisabled ? '#2a2b38' : '#2f3645',
                color: '#f5f5f5',
                border: '1px solid #3d4254',
                cursor: actionDisabled ? 'not-allowed' : 'pointer',
                fontWeight: 600,
              }}
            >
              {testing ? 'Testing…' : 'Test connection'}
            </button>
          </div>
        </section>

        <section style={{ background: 'rgba(255,255,255,0.03)', borderRadius: 16, padding: 20 }}>
          <header style={{ marginBottom: 12 }}>
            <h3 style={{ margin: 0 }}>Status</h3>
            <p style={{ margin: '4px 0 0', color: '#9ea0b5' }}>
              Quick visibility into the active runtime and the last diagnostic response.
            </p>
          </header>
          <div style={{ display: 'grid', gap: 12 }}>
            <div style={{ fontSize: 14, color: '#c2c3d1' }}>
              <strong>Active provider:</strong> {activeProviderId ?? 'Not set'}
              <br />
              <strong>Active model:</strong> {activeModel ?? 'Not set'}
            </div>
            {statusMessage && (
              <div style={{ color: '#7ce38b', fontSize: 14, background: 'rgba(62, 125, 80, 0.25)', padding: '10px 12px', borderRadius: 10 }}>
                {statusMessage}
              </div>
            )}
            {error && (
              <div style={{ color: '#ff9386', fontSize: 14, background: 'rgba(128, 45, 45, 0.35)', padding: '10px 12px', borderRadius: 10 }}>
                {error}
              </div>
            )}
            {lastResponse && (
              <div style={{ fontSize: 13, color: '#a7a9bc', background: '#161723', padding: 12, borderRadius: 12, border: '1px solid #24263a' }}>
                <div style={{ fontSize: 12, color: '#5f6074', marginBottom: 6 }}>
                  {lastResponse.provider_id} · {lastResponse.model}
                </div>
                <div style={{ whiteSpace: 'pre-wrap' }}>{lastResponse.content}</div>
              </div>
            )}
          </div>
        </section>
      </div>
    </div>
  )
}
