import { useEffect, useRef } from 'react'
import { hasHost, onNativeSpectrum } from '../bridge'

const BAND_COUNT = 10
const HISTORY = 10

/* The original FxVisualizer geometry: 100 bars on a 960x120 surface,
 * 4 px wide at a 9.1 px pitch, starting at x=27 and growing from the
 * vertical centre to 100 px. */
const VIRTUAL_WIDTH = 960
const VIRTUAL_HEIGHT = 120
const VIRTUAL_X = 27
const VIRTUAL_PITCH = 9.1
const VIRTUAL_BAR_WIDTH = 4
const VIRTUAL_MAX_HEIGHT = 100

const BAND_BASE = [0.3, 0.56, 0.78, 0.68, 0.48, 0.58, 0.44, 0.31, 0.22, 0.15]

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value))
}

function mockBand(index: number, time: number): number {
  const primary = Math.sin(time * (0.7 + index * 0.055) + index * 1.63)
  const secondary = Math.sin(time * (1.45 + index * 0.035) + index * 0.71)
  const movement = 0.59 + primary * 0.23 + secondary * 0.11
  return clamp(BAND_BASE[index] * movement, 0.025, 0.96)
}

function initialHistory(empty = false): number[][] {
  if (empty) {
    return BAND_BASE.map(() => new Array<number>(HISTORY).fill(0))
  }
  return BAND_BASE.map((base, band) =>
    Array.from({ length: HISTORY }, (_, step) => {
      const shape = 0.55 + 0.32 * Math.sin((band + step) * 0.72)
      return clamp(base * shape, 0.02, 0.9)
    }),
  )
}

/* FxVisualizer::update(): shift both halves outward from the centre,
 * then place the new value in the middle slot. The original order is
 * significant, so keep the read-before-write loop. */
function pushHistory(history: number[][], values: number[]) {
  for (let band = 0; band < BAND_COUNT; band += 1) {
    const row = history[band]
    for (let step = 0; step < HISTORY / 2; step += 1) {
      const next = row[step + 1]
      row[step] = next
      row[HISTORY - 1 - step] = next
    }
    row[HISTORY / 2] = values[band] ?? 0
  }
}

interface SpectrumProps {
  power: boolean
  accentRgb: string
}

function rgba(rgb: string, alpha: number): string {
  return `rgba(${rgb}, ${alpha.toFixed(3)})`
}

function accentEdge(rgb: string): string {
  const [r, g, b] = rgb.split(',').map((part) => Number(part.trim()))
  return `${Math.round(r * 0.82)}, ${Math.round(g * 0.82)}, ${Math.round(b * 0.82)}`
}

function accentCentre(rgb: string): string {
  const [r, g, b] = rgb.split(',').map((part) => Number(part.trim()))
  const lighten = (channel: number) => Math.round(channel + (255 - channel) * 0.16)
  return `${lighten(r)}, ${lighten(g)}, ${lighten(b)}`
}

export function Spectrum({ power, accentRgb }: SpectrumProps) {
  const canvasRef = useRef<HTMLCanvasElement>(null)

  useEffect(() => {
    const canvas = canvasRef.current
    if (!canvas) return
    const context = canvas.getContext('2d')
    if (!context) return

    const native = hasHost()
    const history = initialHistory(native)
    let width = 1
    let height = 1
    let dpr = 1
    let frame = 0
    let lastUpdate = 0
    let time = 2.8
    let lastActive = 0

    const draw = (now = performance.now()) => {
      context.clearRect(0, 0, width, height)
      if (!power) return

      const barWidth = Math.max(
        1,
        (width * VIRTUAL_BAR_WIDTH) / VIRTUAL_WIDTH,
      )
      const pitch = (width * VIRTUAL_PITCH) / VIRTUAL_WIDTH
      const startX = (width * VIRTUAL_X) / VIRTUAL_WIDTH
      const centre = height / 2
      const maxHeight = (height * VIRTUAL_MAX_HEIGHT) / VIRTUAL_HEIGHT
      const live = now - lastActive < 1200
      const alpha = live ? 1 : 0.78

      const gradient = context.createLinearGradient(0, 0, 0, height)
      gradient.addColorStop(0, rgba(accentEdge(accentRgb), alpha))
      gradient.addColorStop(0.5, rgba(accentCentre(accentRgb), alpha))
      gradient.addColorStop(1, rgba(accentEdge(accentRgb), alpha))
      context.fillStyle = gradient

      for (let band = 0; band < BAND_COUNT; band += 1) {
        for (let step = 0; step < HISTORY; step += 1) {
          const value = history[band][step]
          const barHeight = Math.max(1, value * maxHeight)
          const x =
            startX + (band * HISTORY + step) * pitch
          context.fillRect(
            x,
            centre - barHeight / 2,
            barWidth,
            barHeight,
          )
        }
      }
    }

    const resize = () => {
      const rect = canvas.getBoundingClientRect()
      dpr = Math.min(window.devicePixelRatio || 1, 2)
      width = Math.max(1, rect.width)
      height = Math.max(1, rect.height)
      canvas.width = Math.round(width * dpr)
      canvas.height = Math.round(height * dpr)
      context.setTransform(dpr, 0, 0, dpr, 0, 0)
      draw()
    }

    const observer = new ResizeObserver(resize)
    observer.observe(canvas)
    resize()

    let unsubscribe = () => {}

    if (native) {
      unsubscribe = onNativeSpectrum((bands) => {
        if (!power) return
        const values = Array.from({ length: BAND_COUNT }, (_, band) =>
          clamp(
            typeof bands[band] === 'number' ? bands[band] : 0,
            0,
            1,
          ),
        )
        if (values.some((value) => value > 0.02)) lastActive = performance.now()
        pushHistory(history, values)
        draw()
      })
    } else {
      const reducedMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches

      const tick = (timestamp: number) => {
        if (timestamp - lastUpdate > 33) {
          lastUpdate = timestamp
          time += 0.085
          const values = BAND_BASE.map((_, band) => mockBand(band, time))
          lastActive = timestamp
          pushHistory(history, values)
          draw(timestamp)
        }
        frame = requestAnimationFrame(tick)
      }

      if (power && !reducedMotion) {
        frame = requestAnimationFrame(tick)
      }

      const onVisibilityChange = () => {
        if (document.hidden) {
          cancelAnimationFrame(frame)
          frame = 0
        } else if (power && !reducedMotion && frame === 0) {
          lastUpdate = 0
          frame = requestAnimationFrame(tick)
        }
      }

      document.addEventListener('visibilitychange', onVisibilityChange)

      return () => {
        observer.disconnect()
        cancelAnimationFrame(frame)
        unsubscribe()
        document.removeEventListener('visibilitychange', onVisibilityChange)
      }
    }

    return () => {
      observer.disconnect()
      cancelAnimationFrame(frame)
      unsubscribe()
    }
  }, [accentRgb, power])

  return (
    <section className="visualizer" data-live={power} aria-label="Output spectrum">
      <canvas ref={canvasRef} className="spectrum-canvas" aria-hidden="true" />
      {!power ? (
        <div className="visualizer-paused" aria-live="polite">
          <span>Audio processing is off</span>
        </div>
      ) : null}
    </section>
  )
}
