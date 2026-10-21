import { Select } from '@base-ui/react/select'
import { Check, ChevronDown } from 'lucide-react'
import type { OutputOption, PresetOption } from '../data'

interface PresetSelectProps {
  presets: PresetOption[]
  value: string
  dirty: boolean
  disabled: boolean
  onChange: (id: string) => void
}

export function PresetSelect({ presets, value, dirty, disabled, onChange }: PresetSelectProps) {
  const selected = presets.find((preset) => preset.id === value)
  const factory = presets.filter((preset) => preset.group === 'factory')
  const custom = presets.filter((preset) => preset.group === 'custom')

  return (
    <div className="field">
      <div className="field-heading">
        <span className="field-label">Preset</span>
        {dirty ? <span className="edited-badge">Edited</span> : null}
      </div>
      <Select.Root
        value={value}
        onValueChange={(next) => {
          if (typeof next === 'string') onChange(next)
        }}
        disabled={disabled}
      >
        <Select.Trigger className="select-trigger" aria-label="Preset">
          <Select.Value>
            {() => (
              <span className="select-value">
                <span className="select-value-main">{selected?.name ?? 'Select preset'}</span>
                <span className="select-value-detail">
                  {selected?.group === 'custom' ? 'Custom preset' : 'Factory preset'}
                </span>
              </span>
            )}
          </Select.Value>
          <Select.Icon className="select-icon">
            <ChevronDown size={15} strokeWidth={1.9} aria-hidden="true" />
          </Select.Icon>
        </Select.Trigger>

        <Select.Portal>
          <Select.Positioner sideOffset={6} alignItemWithTrigger={false} className="select-positioner">
            <Select.Popup className="select-popup">
              <Select.Group>
                <Select.GroupLabel className="select-group-label">Factory</Select.GroupLabel>
                {factory.map((preset) => (
                  <SelectItem key={preset.id} preset={preset} />
                ))}
              </Select.Group>
              {custom.length > 0 ? (
                <Select.Group>
                  <Select.GroupLabel className="select-group-label">Custom</Select.GroupLabel>
                  {custom.map((preset) => (
                    <SelectItem key={preset.id} preset={preset} />
                  ))}
                </Select.Group>
              ) : null}
            </Select.Popup>
          </Select.Positioner>
        </Select.Portal>
      </Select.Root>
    </div>
  )
}

function SelectItem({ preset }: { preset: PresetOption }) {
  return (
    <Select.Item value={preset.id} label={preset.name} className="select-item">
      <span className="select-item-copy">
        <Select.ItemText className="select-item-name">{preset.name}</Select.ItemText>
        {preset.group === 'custom' ? <span className="select-item-tag">Custom</span> : null}
      </span>
      <Select.ItemIndicator className="select-item-check">
        <Check size={14} strokeWidth={2.2} aria-hidden="true" />
      </Select.ItemIndicator>
    </Select.Item>
  )
}

interface OutputSelectProps {
  options: OutputOption[]
  value: string
  onChange: (id: string) => void
}

export function OutputSelect({ options, value, onChange }: OutputSelectProps) {
  const selected = options.find((option) => option.id === value)

  return (
    <div className="field">
      <div className="field-heading">
        <span className="field-label">Output</span>
      </div>
      <Select.Root
        value={value}
        onValueChange={(next) => {
          if (typeof next === 'string') onChange(next)
        }}
      >
        <Select.Trigger className="select-trigger" aria-label="Output">
          <Select.Value>
            {() => (
              <span className="select-value">
                <span className="select-value-main">{selected?.label ?? 'Select output'}</span>
                <span className="select-value-detail">{selected?.detail ?? ''}</span>
              </span>
            )}
          </Select.Value>
          <Select.Icon className="select-icon">
            <ChevronDown size={15} strokeWidth={1.9} aria-hidden="true" />
          </Select.Icon>
        </Select.Trigger>

        <Select.Portal>
          <Select.Positioner sideOffset={6} alignItemWithTrigger={false} className="select-positioner">
            <Select.Popup className="select-popup select-popup-compact">
              {options.map((option) => (
                <Select.Item
                  key={option.id}
                  value={option.id}
                  label={option.label}
                  className="select-item"
                >
                  <span className="select-item-copy">
                    <Select.ItemText className="select-item-name">{option.label}</Select.ItemText>
                    <span className="select-item-detail">{option.detail}</span>
                  </span>
                  <Select.ItemIndicator className="select-item-check">
                    <Check size={14} strokeWidth={2.2} aria-hidden="true" />
                  </Select.ItemIndicator>
                </Select.Item>
              ))}
            </Select.Popup>
          </Select.Positioner>
        </Select.Portal>
      </Select.Root>
    </div>
  )
}
