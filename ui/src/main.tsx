/**
 * Application bootstrap that mounts the root React tree within the Tauri shell.
 * Keeping this file intentionally small makes it obvious where execution starts
 * when debugging startup issues.
 */
import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './modules/App'

/**
 * Mount the InkOS React application. Strict mode is enabled so lifecycle
 * warnings surface during development.
 */
ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
)
