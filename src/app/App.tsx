import { AppWindow } from 'lucide-react'
import { useState } from 'react'

import { Bootstrap } from '@/console/Bootstrap'
import { Badge } from '@/ui/Badge'
import { Button } from '@/ui/Button'
import { Modal } from '@/ui/Modal'

/**
 * Phase 3 shell — the bootstrap surface front and centre, with the pilot
 * primitives parked behind a modal to keep the carried design system visible
 * until the management-plane layout lands in Phase 4.
 */
export default function App() {
  const [modalOpen, setModalOpen] = useState(false)

  return (
    <div className="flex h-full flex-col">
      <header className="chrome flex h-10 shrink-0 items-center gap-2 border-b border-line px-3">
        <AppWindow size={14} className="text-brand" aria-hidden="true" />
        <span className="text-[12.5px] font-semibold">HarnessLite</span>
        <Badge tone="brand">bootstrap</Badge>
        <div className="ml-auto flex items-center gap-2">
          <Button size="sm" variant="ghost" onClick={() => setModalOpen(true)}>
            Design pilot
          </Button>
        </div>
      </header>
      <main className="min-h-0 flex-1">
        <Bootstrap />
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
          <div className="flex flex-col gap-3 px-4 py-4 text-[12.5px] text-muted">
            <div className="flex items-center gap-2">
              <Badge tone="ok">tokens carried</Badge>
              <Badge tone="warn">layout pending</Badge>
              <Badge>0.1.0</Badge>
            </div>
            If this card renders with the panel border, the pop animation and the
            brand icon tile, the carried design system is live on the new
            baseline.
          </div>
        </Modal>
      )}
    </div>
  )
}
