import { Slider } from '@base-ui/react/slider'
import { EFFECTS } from '../data'

interface EffectRackProps {
  values: number[]
  disabled: boolean
  onChange: (index: number, value: number) => void
  onReset: (index: number) => void
}

export function EffectRack({ values, disabled, onChange, onReset }: EffectRackProps) {
  return (
    <section className={`panel effects-panel ${disabled ? 'is-disabled' : ''}`}>
      <div className="panel-head">
        <div>
          <h2 className="panel-title">Effects</h2>
          <p className="panel-subtitle">Five native DSP controls</p>
        </div>
        <span className="panel-count">5</span>
      </div>

      <div className="effect-list">
        {EFFECTS.map((effect, index) => {
          /* The engine keeps fractional values from .fac presets, but the
           * native controls use a 1.0 step and round their readout. Match
           * that here instead of spilling decimals into a narrow column. */
          const value = Math.round(values[index] ?? 0)
          return (
            <div
              className="effect-row"
              key={effect.id}
              onDoubleClick={() => onReset(index)}
              title="Double-click to reset"
            >
              <div className="effect-name-wrap" title={effect.description}>
                <span className="effect-name">{effect.name}</span>
              </div>

              <Slider.Root
                className="slider effect-slider"
                value={value}
                min={0}
                max={10}
                step={1}
                disabled={disabled}
                onValueChange={(next) => onChange(index, next)}
              >
                <Slider.Control className="slider-control">
                  <Slider.Track className="slider-track">
                    <Slider.Indicator className="slider-indicator" />
                  </Slider.Track>
                  <Slider.Thumb
                    className="slider-thumb"
                    aria-label={`${effect.name} level`}
                    getAriaValueText={(formatted) => `${formatted} out of 10`}
                  />
                </Slider.Control>
              </Slider.Root>

              <span className="effect-value" aria-hidden="true">
                {value}
              </span>
            </div>
          )
        })}
      </div>

      <p className="panel-hint">Double-click a slider to reset it.</p>
    </section>
  )
}
