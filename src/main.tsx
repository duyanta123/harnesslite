import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

import App from '@/app/App'
import { installIpcMock } from '@/dev/mock-ipc'
import { behaveLikeAWindow } from '@/lib/native'
import { installCrashEvidence } from '@/lib/crash'
import '@/styles/app.css'

// Stop the webview behaving like a browser tab before anything renders, and
// capture crash evidence from the first line on.
behaveLikeAWindow()
installCrashEvidence()

// Browser self-checks (no desktop shell) answer IPC with fixture data.
if (!('__TAURI_INTERNALS__' in window)) {
  installIpcMock()
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
