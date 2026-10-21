import { Dialog } from '@base-ui/react/dialog'
import { Switch } from '@base-ui/react/switch'
import { Toggle } from '@base-ui/react/toggle'
import { ToggleGroup } from '@base-ui/react/toggle-group'
import { Check, Download, Palette, Save, Upload, X } from 'lucide-react'
import {
  ACCENTS,
  type AccentId,
  type ThemeMode,
} from '../data'

interface SettingsDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  theme: ThemeMode
  onThemeChange: (theme: ThemeMode) => void
  accentId: AccentId
  accentHex: string
  onAccentChange: (accent: string) => void
  autostart: boolean
  onAutostartChange: (enabled: boolean) => void
  onImport: () => void
  onExport: () => void
  onSavePreset: () => void
  onQuit: () => void
}

export function SettingsDialog({
  open,
  onOpenChange,
  theme,
  onThemeChange,
  accentId,
  accentHex,
  onAccentChange,
  autostart,
  onAutostartChange,
  onImport,
  onExport,
  onSavePreset,
  onQuit,
}: SettingsDialogProps) {
  const customAccent = accentId === 'custom'
  const accentLabel = customAccent
    ? 'Custom'
    : (ACCENTS.find((accent) => accent.id === accentId)?.label ?? 'Custom')

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Backdrop className="dialog-backdrop" />
        <Dialog.Viewport className="dialog-viewport">
          <Dialog.Popup className="dialog settings-dialog">
            <header className="dialog-head">
              <div>
                <Dialog.Title className="dialog-title">Settings</Dialog.Title>
                <Dialog.Description className="dialog-description">
                  Appearance, presets, and startup behavior.
                </Dialog.Description>
              </div>
              <Dialog.Close className="icon-button" aria-label="Close settings">
                <X size={15} strokeWidth={1.9} aria-hidden="true" />
              </Dialog.Close>
            </header>

            <div className="settings-body">
              <section className="settings-section">
                <div className="settings-section-head">
                  <h3>Appearance</h3>
                  <span>{accentLabel}</span>
                </div>

                <div className="settings-row">
                  <div className="settings-row-copy">
                    <span className="settings-row-title">Theme</span>
                    <span className="settings-row-description">
                      Soft light or low-contrast dark surfaces.
                    </span>
                  </div>
                  <ToggleGroup
                    className="segmented-control"
                    value={[theme]}
                    onValueChange={(values) => {
                      const next = values[values.length - 1]
                      if (next === 'dark' || next === 'light') onThemeChange(next)
                    }}
                    aria-label="Theme"
                  >
                    <Toggle value="dark" className="segment">
                      Dark
                    </Toggle>
                    <Toggle value="light" className="segment">
                      Light
                    </Toggle>
                  </ToggleGroup>
                </div>

                <div className="settings-row">
                  <div className="settings-row-copy">
                    <span className="settings-row-title">Accent</span>
                    <span className="settings-row-description">
                      Used for controls and the live spectrum.
                    </span>
                  </div>
                  <div className="accent-controls">
                  <ToggleGroup
                    className="swatch-list"
                    value={[accentId]}
                    onValueChange={(values) => {
                      const next = values[values.length - 1] as AccentId | undefined
                      const choice = ACCENTS.find((accent) => accent.id === next)
                      if (choice) onAccentChange(choice.value)
                    }}
                    aria-label="Accent color"
                  >
                    {ACCENTS.map((choice) => (
                      <Toggle
                        key={choice.id}
                        value={choice.id}
                        className="swatch-button"
                        aria-label={choice.label}
                        title={choice.label}
                      >
                        <span
                          className="swatch-dot"
                          style={{ background: choice.value }}
                        />
                        <Check
                          size={11}
                          strokeWidth={3}
                          className="swatch-check"
                          aria-hidden="true"
                        />
                      </Toggle>
                    ))}
                  </ToggleGroup>
                  <label
                    className={`swatch-button swatch-button-custom ${
                      customAccent ? 'is-pressed' : ''
                    }`}
                    title="Custom color"
                  >
                    <span
                      className="swatch-dot swatch-custom"
                      style={
                        customAccent
                          ? { background: accentHex }
                          : undefined
                      }
                    >
                      {customAccent ? (
                        <Check
                          size={11}
                          strokeWidth={3}
                          className="swatch-custom-check"
                          aria-hidden="true"
                        />
                      ) : (
                        <Palette size={11} strokeWidth={1.9} aria-hidden="true" />
                      )}
                    </span>
                    <input
                      type="color"
                      value={accentHex}
                      onChange={(event) => onAccentChange(event.target.value)}
                      aria-label="Custom accent color"
                    />
                  </label>
                  </div>
                </div>
              </section>

              <section className="settings-section">
                <div className="settings-section-head">
                  <h3>Presets</h3>
                  <span>Factory files stay untouched</span>
                </div>
                <p className="settings-copy">
                  Save the current effects and EQ as a custom preset, or move
                  presets in and out as .fac files.
                </p>
                <div className="settings-actions">
                  <button type="button" className="action-button" onClick={onImport}>
                    <Upload size={14} strokeWidth={1.9} aria-hidden="true" />
                    Import
                  </button>
                  <button type="button" className="action-button" onClick={onExport}>
                    <Download size={14} strokeWidth={1.9} aria-hidden="true" />
                    Export
                  </button>
                  <button
                    type="button"
                    className="action-button action-button-accent"
                    onClick={onSavePreset}
                  >
                    <Save size={14} strokeWidth={1.9} aria-hidden="true" />
                    Save preset
                  </button>
                </div>
              </section>

              <section className="settings-section settings-section-last">
                <div className="settings-row">
                  <div className="settings-row-copy">
                    <span className="settings-row-title">Start at login</span>
                    <span className="settings-row-description">
                      Keep the engine ready in the background.
                    </span>
                  </div>
                  <Switch.Root
                    className="settings-switch"
                    checked={autostart}
                    onCheckedChange={onAutostartChange}
                    aria-label="Start at login"
                  >
                    <span className="switch-track" aria-hidden="true">
                      <Switch.Thumb className="switch-thumb" />
                    </span>
                  </Switch.Root>
                </div>
              </section>
            </div>

            <footer className="settings-footer">
              <div className="settings-version">
                <span>SauceFX 0.2.0</span>
                <span className="meta-dot" aria-hidden="true" />
                <span>AGPL-3.0</span>
              </div>
              <button type="button" className="quit-button" onClick={onQuit}>
                Quit SauceFX
              </button>
            </footer>
          </Dialog.Popup>
        </Dialog.Viewport>
      </Dialog.Portal>
    </Dialog.Root>
  )
}
