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

function deviceDisplay(raw: string): { label: string; detail: string } {
  const separator = raw.indexOf(' — ')
  const port = separator >= 0 ? raw.slice(0, separator) : ''
  const device = (separator >= 0 ? raw.slice(separator + 3) : raw)
    .replace(/\s+(?:Analog (?:Stereo|Mono)|Digital Stereo(?: \([^)]*\))?|(?:Analog|Digital) Surround .+)$/i, '')
  if (/^(?:alsa_output|bluez_output)\./.test(device))
    return { label: device.startsWith('alsa_output.usb-') ? 'USB audio device'
      : device.startsWith('bluez_output.') ? 'Bluetooth audio device' : 'Audio device', detail: 'Audio output' }
  const builtIn = /^built[- ]in audio$/i.test(device)
  if (builtIn && (!port || /^(?:Line Out|Speakers?|Analog Output)$/i.test(port)))
    return { label: 'Speakers', detail: 'Built-in audio' }
  if (builtIn && /^Headphones?$/i.test(port))
    return { label: 'Headphones', detail: 'Built-in headphone jack' }
  if (/HDMI|DisplayPort/i.test(port))
    return { label: port, detail: builtIn ? 'Monitor or TV audio' : device }
  if (!port || /^(?:Analog Output|Analog Stereo Output|Output)$/i.test(port))
    return { label: device, detail: 'Audio output' }
  return { label: builtIn ? port : `${device} — ${port}`, detail: builtIn ? 'Built-in audio' : 'Audio output' }
}

function outputDisplay(option: OutputOption): OutputOption {
  if (option.id === 'system')
    return { ...option, label: 'Direct playback', detail: 'Use system output without SauceFX' }
  if (option.id === 'saucefx') {
    const detail = option.detail.startsWith('Waiting for')
      ? 'No audio device connected'
      : option.detail === 'Follows connected audio devices'
        ? 'Chooses a connected headset or speakers'
        : `Using ${deviceDisplay(option.detail).label} · switches automatically`
    return { ...option, label: 'Automatic', detail }
  }

  const display = deviceDisplay(option.label)
  // Connection type comes from the generic sink identity, never a device brand.
  let connection = ''
  try {
    const name = atob(option.id.split(':')[1] ?? '')
    if (name.startsWith('alsa_output.usb-')) connection = 'USB audio'
    else if (name.startsWith('bluez_output.')) connection = 'Bluetooth audio'
  } catch {
    // Browser preview IDs and future backends may use another identity format.
  }
  if (/^(?:alsa_output|bluez_output)\./.test(option.label))
    display.label = connection === 'USB audio' ? 'Saved USB device'
      : connection === 'Bluetooth audio' ? 'Saved Bluetooth device' : 'Saved audio device'
  if (connection) display.detail = connection
  if (option.detail.startsWith('Disconnected · ')) {
    const fallback = option.detail.slice('Disconnected · '.length)
    display.detail = fallback.startsWith('Waiting for')
      ? 'Not connected · no other audio device available'
      : `Not connected · using ${deviceDisplay(fallback).label}`
  } else if (option.available === false)
    display.detail = display.label === 'Headphones' ? 'Plug in wired headphones to use' : 'Connect this device to use'
  return { ...option, ...display }
}

export function OutputSelect({ options, value, onChange }: OutputSelectProps) {
  const displayed = options.map(outputDisplay)
  const selected = displayed.find((option) => option.id === value)
  const groups = [
    { label: 'With SauceFX effects', options: displayed.filter((option) => option.id !== 'system' && option.available !== false) },
    { label: 'Not connected', options: displayed.filter((option) => option.id !== 'system' && option.available === false) },
    { label: 'Without effects', options: displayed.filter((option) => option.id === 'system') },
  ]

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
              {groups.filter((group) => group.options.length > 0).map((group) => (
                <Select.Group key={group.label}>
                  <Select.GroupLabel className="select-group-label">{group.label}</Select.GroupLabel>
                  {group.options.map((option) => (
                    <Select.Item
                      key={option.id}
                      value={option.id}
                      label={option.label}
                      disabled={option.available === false}
                      title={option.label}
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
                </Select.Group>
              ))}
            </Select.Popup>
          </Select.Positioner>
        </Select.Portal>
      </Select.Root>
    </div>
  )
}
