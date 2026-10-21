import { useState } from 'react'
import { Slider } from '@base-ui/react/slider'
import { RotateCcw } from 'lucide-react'
import { IconButton } from './IconButton'
import {
  EQ_FREQUENCIES,
  EQ_LABEL_INDICES,
  EQ_MAX,
  formatFrequency,
  formatGain,
} from '../data'

const VIEW_WIDTH = 1000
const VIEW_HEIGHT = 200
const TOP = 12
const BOTTOM = 188

interface Point {
  x: number
  y: number
}

function gainToY(value: number): number {
  const clamped = Math.max(-EQ_MAX, Math.min(EQ_MAX, value))
  return TOP + ((EQ_MAX - clamped) / (EQ_MAX * 2)) * (BOTTOM - TOP)
}

function curvePoints(values: number[]): Point[] {
  return values.map((value, index) => ({
    x: ((index + 0.5) / values.length) * VIEW_WIDTH,
    y: gainToY(value),
  }))
}

function smoothPath(points: Point[]): string {
  if (points.length === 0) return ''
  let path = `M ${points[0].x.toFixed(2)} ${points[0].y.toFixed(2)}`
  for (let index = 0; index < points.length - 1; index += 1) {
    const previous = points[index - 1] ?? points[index]
    const current = points[index]
    const next = points[index + 1]
    const after = points[index + 2] ?? next

    const control1X = current.x + (next.x - previous.x) / 6
    const control1Y = current.y + (next.y - previous.y) / 6
    const control2X = next.x - (after.x - current.x) / 6
    const control2Y = next.y - (after.y - current.y) / 6

    path += ` C ${control1X.toFixed(2)} ${control1Y.toFixed(2)}, ${control2X.toFixed(
      2,
    )} ${control2Y.toFixed(2)}, ${next.x.toFixed(2)} ${next.y.toFixed(2)}`
  }
  return path
}

interface EqualizerProps {
  values: number[]
  disabled: boolean
  onChange: (index: number, value: number) => void
  onReset: (index: number) => void
  onResetAll: () => void
}

export function Equalizer({
  values,
  disabled,
  onChange,
  onReset,
  onResetAll,
}: EqualizerProps) {
  const [activeBand, setActiveBand] = useState<number | null>(null)

  const points = curvePoints(values)
  const linePath = smoothPath(points)
  const fillPath = `${linePath} L ${VIEW_WIDTH} ${BOTTOM} L 0 ${BOTTOM} Z`
  const activeFrequency = activeBand === null ? null : EQ_FREQUENCIES[activeBand]
  const activeValue = activeBand === null ? null : values[activeBand]

  return (
    <section className={`panel eq-panel ${disabled ? 'is-disabled' : ''}`}>
      <div className="panel-head eq-panel-head">
        <div>
          <h2 className="panel-title">Equalizer</h2>
          <p className="panel-subtitle">
            {activeFrequency !== null && activeValue !== null ? (
              <>
                {formatFrequency(activeFrequency)} ·{' '}
                <span className="eq-active-value">{formatGain(activeValue)} dB</span>
              </>
            ) : (
              '31 bands · ±12 dB'
            )}
          </p>
        </div>
        <IconButton label="Reset equalizer" onClick={onResetAll} disabled={disabled}>
          <RotateCcw size={14} strokeWidth={1.9} aria-hidden="true" />
        </IconButton>
      </div>

      <div className="eq-plot">
        <div className="eq-chart">
          <svg
            className="eq-svg"
            viewBox={`0 0 ${VIEW_WIDTH} ${VIEW_HEIGHT}`}
            preserveAspectRatio="none"
            aria-hidden="true"
          >
            <defs>
              <linearGradient id="eq-fill" x1="0" y1="0" x2="0" y2="1">
                <stop offset="0%" stopColor="var(--accent)" stopOpacity="0.15" />
                <stop offset="100%" stopColor="var(--accent)" stopOpacity="0.005" />
              </linearGradient>
            </defs>

            {[0.2, 0.4, 0.6, 0.8].map((position) => (
              <line
                key={position}
                x1={position * VIEW_WIDTH}
                y1={TOP}
                x2={position * VIEW_WIDTH}
                y2={BOTTOM}
                className="eq-grid-line eq-grid-line-vertical"
              />
            ))}

            {[TOP, 56, 100, 144, BOTTOM].map((y, index) => (
              <line
                key={y}
                x1="0"
                y1={y}
                x2={VIEW_WIDTH}
                y2={y}
                className={`eq-grid-line ${index === 2 ? 'eq-grid-line-zero' : ''}`}
              />
            ))}

            <path d={fillPath} fill="url(#eq-fill)" />
            <path
              d={linePath}
              className="eq-curve"
              vectorEffect="non-scaling-stroke"
            />
          </svg>

          <div className="eq-sliders">
            {values.map((value, index) => (
              <div
                className="eq-band"
                key={EQ_FREQUENCIES[index]}
                onPointerEnter={() => setActiveBand(index)}
                onPointerLeave={() => setActiveBand(null)}
                onContextMenu={(event) => {
                  event.preventDefault()
                  onReset(index)
                }}
              >
                <Slider.Root
                  orientation="vertical"
                  value={value}
                  min={-EQ_MAX}
                  max={EQ_MAX}
                  step={1}
                  disabled={disabled}
                  onValueChange={(next) => onChange(index, next)}
                >
                  <Slider.Control className="eq-control">
                    <Slider.Track className="eq-track">
                      <Slider.Indicator className="eq-indicator" />
                    </Slider.Track>
                    <Slider.Thumb
                      className="eq-thumb"
                      getAriaLabel={() =>
                        `${formatFrequency(EQ_FREQUENCIES[index])} band`
                      }
                      getAriaValueText={(_, next) => `${next} decibels`}
                      onFocus={() => setActiveBand(index)}
                      onBlur={() => setActiveBand(null)}
                    />
                  </Slider.Control>
                </Slider.Root>
              </div>
            ))}
          </div>
        </div>

        <div className="eq-frequency-row" aria-hidden="true">
          {EQ_LABEL_INDICES.map((index) => (
            <span
              key={EQ_FREQUENCIES[index]}
              className="eq-frequency-label"
              style={{ left: `${((index + 0.5) / EQ_FREQUENCIES.length) * 100}%` }}
            >
              {formatFrequency(EQ_FREQUENCIES[index], true)}
            </span>
          ))}
        </div>

        <div className="eq-scale" aria-hidden="true">
          <span>+12</span>
          <span>0</span>
          <span>−12</span>
        </div>
      </div>

    </section>
  )
}
