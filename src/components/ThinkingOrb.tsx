import { useEffect, useRef } from 'react'

import type { Tone } from '@/lib/status'

/**
 * The agent, as a small light that is alive.
 *
 * Adapted from the "thinking orb" pattern: a core and a cloud of particles on
 * two tilted rings, drawn on a canvas so the depth cue is real — particles on
 * the back half of the ring are dimmer and smaller than the ones in front —
 * rather than faked with opacity alone.
 *
 * It reads the same `Tone` the status dot does, so one phase is one colour
 * everywhere, and what the orb adds is tempo: a transient phase (`starting`,
 * `restarting`) sends the particles round noticeably faster and breathes the
 * core, a settled one drifts, a failed one flickers, and a stopped one barely
 * moves at all. A supervisor's most-asked question is "is it doing anything
 * right now", and tempo answers it from across the room.
 *
 * `prefers-reduced-motion` gets a single still frame at t=0 rather than the
 * loop; the frame is a real one, so the indicator never disappears.
 */

/** Particles per ring. Tuned for the sizes this ships at (12–20px). */
const RING_A = 6
const RING_B = 8

const resolveColor = (value: string): string => {
  const match = /^var\((.+)\)$/.exec(value.trim())
  const name = match?.[1]
  if (name === undefined) return value
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || value
}

export function ThinkingOrb({ tone, size = 16 }: { tone: Tone; size?: number }) {
  const canvas = useRef<HTMLCanvasElement>(null)

  useEffect(() => {
    const element = canvas.current
    const context = element?.getContext('2d')
    if (!element || !context) return

    // 2x is where the difference stops being visible; a 4K-scale buffer for a
    // 16px indicator is memory nobody asked for.
    const dpr = Math.min(window.devicePixelRatio || 1, 2)
    element.width = size * dpr
    element.height = size * dpr

    // The tone carries its colour as a custom property, and the property's
    // value moves when the theme does — so it is re-read whenever the theme
    // attribute on the root flips, which is the only way it changes.
    let color = resolveColor(tone.color)
    const themeWatch = new MutationObserver(() => {
      color = resolveColor(tone.color)
    })
    themeWatch.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['data-theme'],
    })

    const resting = tone.color === 'var(--color-faint)'
    const failed = tone.color === 'var(--color-danger)'

    const draw = (time: number) => {
      const t = time / 1000
      context.setTransform(dpr, 0, 0, dpr, 0, 0)
      context.clearRect(0, 0, size, size)

      const cx = size / 2
      const cy = size / 2

      // The core: a filled disc with a soft bloom, breathing faster while the
      // phase is transient. A flicker rides on a failed phase — quiet enough to
      // stay pleasant, visible enough to read as "not settled".
      const pulse = tone.live ? 1 + 0.1 * Math.sin(t * 5.2) : 1 + 0.04 * Math.sin(t * 1.6)
      const flicker = failed ? 0.82 + 0.18 * Math.sin(t * 9.3) : 1
      const coreRadius = size * 0.15 * pulse
      context.beginPath()
      context.arc(cx, cy, Math.max(coreRadius, 1), 0, Math.PI * 2)
      context.fillStyle = color
      context.globalAlpha = (resting ? 0.45 : 0.9) * flicker
      context.shadowColor = color
      context.shadowBlur = size * 0.38
      context.fill()
      context.shadowBlur = 0

      // The rings: each particle rides a circle squashed into an ellipse and
      // tilted in the plane, which is what turns N dots into a sphere. The sine
      // of the un-squashed angle is the depth, and depth drives both size and
      // alpha — that is the half of the illusion that sells the other half.
      const tempo = tone.live ? 1.1 : resting ? 0.1 : 0.3
      const drawRing = (
        count: number,
        ringRadius: number,
        tilt: number,
        phase: number,
        direction: 1 | -1,
      ) => {
        const squash = 0.42
        for (let i = 0; i < count; i++) {
          const angle = phase + direction * t * tempo + (i / count) * Math.PI * 2
          const depth = Math.sin(angle) // -1 back … +1 front
          const ex = Math.cos(angle) * ringRadius
          const ey = Math.sin(angle) * ringRadius * squash
          // Tilt the ellipse plane so the two rings do not read as one track.
          const x = cx + ex * Math.cos(tilt) - ey * Math.sin(tilt)
          const y = cy + ex * Math.sin(tilt) + ey * Math.cos(tilt)

          const near = 0.55 + 0.45 * ((depth + 1) / 2)
          const radius = Math.max(size * 0.045 * near, 0.7)
          context.beginPath()
          context.arc(x, y, radius, 0, Math.PI * 2)
          context.fillStyle = color
          context.globalAlpha = (resting ? 0.3 : 0.62) * near * flicker
          context.fill()
        }
      }

      drawRing(RING_A, size * 0.3, -0.55, 0, 1)
      drawRing(RING_B, size * 0.46, 0.65, Math.PI / RING_B, -1)
      context.globalAlpha = 1
    }

    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)')
    let raf = 0
    const loop = (time: number) => {
      draw(time)
      raf = requestAnimationFrame(loop)
    }
    if (reduced.matches) draw(0)
    else raf = requestAnimationFrame(loop)

    return () => {
      cancelAnimationFrame(raf)
      themeWatch.disconnect()
    }
  }, [tone, size])

  return (
    <canvas
      ref={canvas}
      aria-hidden="true"
      style={{ width: size, height: size }}
      className="shrink-0"
    />
  )
}
