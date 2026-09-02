import { AppWindow } from 'lucide-react'

import { Badge } from '@/ui/Badge'
import { Button } from '@/ui/Button'
import { Modal } from '@/ui/Modal'
import { useState } from 'react'

/**
 * Phase 0 shell — a pilot surface for the carried design tokens and
 * primitives. It exists to prove the carried component layer renders on the
 * HarnessLite baseline; the real conversation-first layout lands in Phase 4.
 */
export default function App() {
  const [modalOpen, setModalOpen] = useState(false)

  return (
    <div className="flex h-full flex-col">
      <header className="chrome flex h-10 shrink-0 items-center gap-2 border-b border-line px-3">
        <AppWindow size={14} className="text-brand" aria-hidden="true" />
        <span className="text-[12.5px] font-semibold">HarnessLite</span>
        <Badge tone="brand">Phase 0</Badge>
      </header>
      <main className="flex min-h-0 flex-1 flex-col items-center justify-center gap-3">
        <p className="caption">scaffold online · tokens · primitives</p>
        <div className="flex items-center gap-2">
          <Badge tone="ok">tokens carried</Badge>
          <Badge tone="warn">layout pending</Badge>
          <Badge>0.1.0</Badge>
        </div>
        <div className="flex items-center gap-2">
          <Button onClick={() => setModalOpen(true)}>Open pilot modal</Button>
          <Button variant="secondary" onClick={() => setModalOpen(true)}>
            Secondary
          </Button>
          <Button variant="ghost">Ghost</Button>
          <Button variant="danger">Danger</Button>
        </div>
      </main>

      {modalOpen && (
        <Modal
          icon={AppWindow}
          title="HarnessLite pilot modal"
          subtitle="Modal / Button / Badge on the carried token system."
          closeLabel="Close"
          onClose={() => setModalOpen(false)}
          footer={
            <div className="flex justify-end">
              <Button onClick={() => setModalOpen(false)}>Got it</Button>
            </div>
          }
        >
          <div className="px-4 py-4 text-[12.5px] text-muted">
            If this card renders with the panel border, the pop animation and the
            brand icon tile, the carried design system is live on the new
            baseline.
          </div>
        </Modal>
      )}
    </div>
  )
}
