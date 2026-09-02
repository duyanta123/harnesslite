import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'

import App from '@/app/App'
import { installIpcMock } from '@/dev/mock-ipc'
import '@/ui/app.css'

// Browser self-checks (no desktop shell) answer IPC with fixture data.
if (!('__TAURI_INTERNALS__' in window)) {
  installIpcMock()
}

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
)
