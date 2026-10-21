import { useEffect, useMemo, useState } from 'react'
import { AppHeader } from './components/AppHeader'
import { EffectRack } from './components/EffectRack'
import { Equalizer } from './components/Equalizer'
import { SavePresetDialog } from './components/SavePresetDialog'
import { PresetSelect, OutputSelect } from './components/Selectors'
import { SettingsDialog } from './components/SettingsDialog'
import { Spectrum } from './components/Spectrum'
import { Toast } from './components/Toast'
import {
  hasHost,
  hostCommand,
  initialHostValue,
  onNativeState,
  onNativeToast,
  type NativeState,
} from './bridge'
import {
  ACCENT_STORAGE_KEY,
  ACCENTS,
  CUSTOM_PRESETS_STORAGE_KEY,
  FACTORY_PRESETS,
  OUTPUTS,
  THEME_STORAGE_KEY,
  accentIdFor,
  getAccent,
  type AccentId,
  type Preset,
  type PresetOption,
  type ThemeMode,
} from './data'

const DEFAULT_PRESET_ID = 'music'
const INITIAL_PRESET =
  FACTORY_PRESETS.find((preset) => preset.id === DEFAULT_PRESET_ID) ??
  FACTORY_PRESETS[0]

function readStorage(key: string): string | null {
  try {
    return window.localStorage.getItem(key)
  } catch {
    return null
  }
}

function previewParam(name: string): string | null {
  try {
    return new URLSearchParams(window.location.search).get(name)
  } catch {
    return null
  }
}

function writeStorage(key: string, value: string): void {
  try {
    window.localStorage.setItem(key, value)
  } catch {
    // Local storage can be unavailable when the UI is loaded from a
    // restricted WebKit origin. The session still works in memory.
  }
}

function loadTheme(): ThemeMode {
  const preview = previewParam('theme')
  if (preview === 'light' || preview === 'dark') return preview
  const host = initialHostValue('saucefxTheme')
  if (host === 'light' || host === 'dark') return host
  return readStorage(THEME_STORAGE_KEY) === 'light' ? 'light' : 'dark'
}

function validHex(value: string | null): value is string {
  return value !== null && /^#[0-9a-f]{6}$/i.test(value)
}

function loadAccentHex(): string {
  const preview = previewParam('accent')
  if (preview) {
    const preset = getAccent(preview as AccentId)
    if (preset) return preset.value
    if (validHex(preview)) return preview
  }
  const host = initialHostValue('saucefxAccent')
  if (validHex(host)) return host
  const stored = readStorage(ACCENT_STORAGE_KEY)
  if (stored) {
    const preset = getAccent(stored as AccentId)
    if (preset) return preset.value
    if (validHex(stored)) return stored
  }
  return ACCENTS[0].value
}

function loadCustomPresets(): Preset[] {
  const stored = readStorage(CUSTOM_PRESETS_STORAGE_KEY)
  if (!stored) return []
  try {
    const parsed: unknown = JSON.parse(stored)
    if (!Array.isArray(parsed)) return []
    return parsed
      .filter((entry): entry is Preset => {
        if (typeof entry !== 'object' || entry === null) return false
        const candidate = entry as Partial<Preset>
        return (
          typeof candidate.id === 'string' &&
          typeof candidate.name === 'string' &&
          Array.isArray(candidate.effects) &&
          candidate.effects.length === 5 &&
          candidate.effects.every((value) => typeof value === 'number') &&
          Array.isArray(candidate.eq) &&
          candidate.eq.length === 31 &&
          candidate.eq.every((value) => typeof value === 'number')
        )
      })
      .map((preset) => ({ ...preset, group: 'custom' as const }))
  } catch {
    return []
  }
}

function arraysEqual(first: number[], second: number[]): boolean {
  return (
    first.length === second.length &&
    first.every((value, index) => value === second[index])
  )
}

function numberArray(value: unknown, length: number): number[] {
  const source = Array.isArray(value) ? value : []
  return Array.from({ length }, (_, index) => {
    const entry = source[index]
    return typeof entry === 'number' && Number.isFinite(entry) ? entry : 0
  })
}

function presetOptionsFrom(value: unknown): PresetOption[] | null {
  if (!Array.isArray(value)) return null
  const options = value.flatMap((entry): PresetOption[] => {
    if (typeof entry !== 'object' || entry === null) return []
    const candidate = entry as Partial<PresetOption>
    if (
      typeof candidate.id !== 'string' ||
      typeof candidate.name !== 'string' ||
      (candidate.group !== 'factory' && candidate.group !== 'custom')
    ) {
      return []
    }
    return [
      {
        id: candidate.id,
        name: candidate.name,
        group: candidate.group,
      },
    ]
  })
  return options.length > 0 ? options : null
}

function rgbTriplet(hex: string): string {
  const normalized = hex.replace('#', '')
  const value = Number.parseInt(normalized, 16)
  return `${(value >> 16) & 255}, ${(value >> 8) & 255}, ${value & 255}`
}

function App() {
  const [theme, setTheme] = useState<ThemeMode>(loadTheme)
  const [accentHex, setAccentHex] = useState<string>(loadAccentHex)
  const [power, setPower] = useState(true)
  const [output, setOutput] = useState('system')
  const [previewCustomPresets, setPreviewCustomPresets] = useState<Preset[]>(
    loadCustomPresets,
  )
  const [nativePresets, setNativePresets] = useState<PresetOption[] | null>(null)
  const [presetId, setPresetId] = useState(DEFAULT_PRESET_ID)
  const [effects, setEffects] = useState<number[]>([...INITIAL_PRESET.effects])
  const [eq, setEq] = useState<number[]>([...INITIAL_PRESET.eq])
  const [baselineEffects, setBaselineEffects] = useState<number[]>([
    ...INITIAL_PRESET.effects,
  ])
  const [baselineEq, setBaselineEq] = useState<number[]>([...INITIAL_PRESET.eq])
  const [dirty, setDirty] = useState(false)
  const [autostart, setAutostart] = useState(false)
  const [settingsOpen, setSettingsOpen] = useState(
    () => previewParam('panel') === 'settings',
  )
  const [saveOpen, setSaveOpen] = useState(() => previewParam('panel') === 'save')
  const [toast, setToast] = useState<{ text: string; id: number } | null>(null)

  const presetOptions = useMemo<PresetOption[]>(() => {
    if (nativePresets) return nativePresets
    return [...FACTORY_PRESETS, ...previewCustomPresets].map(
      ({ id, name, group }) => ({ id, name, group }),
    )
  }, [nativePresets, previewCustomPresets])

  const selectedPreset =
    presetOptions.find((preset) => preset.id === presetId) ??
    presetOptions[0] ?? {
      id: DEFAULT_PRESET_ID,
      name: INITIAL_PRESET.name,
      group: 'factory' as const,
    }
  const accentId = accentIdFor(accentHex)
  const accentRgb = rgbTriplet(accentHex)

  useEffect(() => {
    document.documentElement.dataset.theme = theme
    document.documentElement.style.colorScheme = theme
    writeStorage(THEME_STORAGE_KEY, theme)
  }, [theme])

  useEffect(() => {
    const root = document.documentElement
    root.style.setProperty('--accent', accentHex)
    root.style.setProperty('--accent-rgb', accentRgb)
    writeStorage(ACCENT_STORAGE_KEY, accentHex)
  }, [accentHex, accentRgb])

  useEffect(() => {
    writeStorage(CUSTOM_PRESETS_STORAGE_KEY, JSON.stringify(previewCustomPresets))
  }, [previewCustomPresets])

  useEffect(() => {
    if (!toast) return
    const timer = window.setTimeout(() => setToast(null), 2800)
    return () => window.clearTimeout(timer)
  }, [toast])

  useEffect(() => {
    if (!hasHost()) return

    const unsubscribeState = onNativeState((state: NativeState) => {
      const nextPresets = presetOptionsFrom(state.presets)
      if (nextPresets) setNativePresets(nextPresets)

      const nextEffects = numberArray(state.effects, 5)
      const nextEq = numberArray(state.eq, 31)
      setPower(Boolean(state.power))
      setOutput(state.output === 'saucefx' ? 'saucefx' : 'system')
      setPresetId(
        typeof state.presetId === 'string' && state.presetId.length > 0
          ? state.presetId
          : DEFAULT_PRESET_ID,
      )
      setEffects(nextEffects)
      setEq(nextEq)
      setBaselineEffects(nextEffects)
      setBaselineEq(nextEq)
      setDirty(false)
      setAutostart(Boolean(state.autostart))
      setTheme(state.theme === 'light' ? 'light' : 'dark')
      setAccentHex(
        state.customAccent && validHex(state.accent)
          ? state.accent
          : ACCENTS[0].value,
      )
    })

    const unsubscribeToast = onNativeToast((message) => {
      if (typeof message === 'string' && message.length > 0) showToast(message)
    })

    hostCommand('ready')

    return () => {
      unsubscribeState()
      unsubscribeToast()
    }
  }, [])

  function showToast(text: string) {
    setToast({ text, id: Date.now() })
  }

  function updateDirty(nextEffects: number[], nextEq: number[]) {
    setDirty(
      !arraysEqual(nextEffects, baselineEffects) ||
        !arraysEqual(nextEq, baselineEq),
    )
  }

  function handlePowerChange(nextPower: boolean) {
    setPower(nextPower)
    hostCommand('power', { on: nextPower })
  }

  function handlePresetChange(id: string) {
    const preset = presetOptions.find((item) => item.id === id)
    if (!preset) return
    setPresetId(preset.id)
    setDirty(false)

    if (!hasHost()) {
      const preview =
        FACTORY_PRESETS.find((item) => item.id === id) ??
        previewCustomPresets.find((item) => item.id === id)
      if (preview) {
        setEffects([...preview.effects])
        setEq([...preview.eq])
        setBaselineEffects([...preview.effects])
        setBaselineEq([...preview.eq])
      }
    }

    hostCommand('preset', { id: preset.id, name: preset.name })
  }

  function handleOutputChange(id: string) {
    setOutput(id)
    hostCommand('output', { id })
  }

  function handleEffectChange(index: number, value: number) {
    const nextEffects = [...effects]
    nextEffects[index] = value
    setEffects(nextEffects)
    updateDirty(nextEffects, eq)
    hostCommand('effect', { index, value })
  }

  function handleEffectReset(index: number) {
    const nextEffects = [...effects]
    nextEffects[index] = baselineEffects[index] ?? 0
    setEffects(nextEffects)
    updateDirty(nextEffects, eq)
    hostCommand('effect', { index, value: nextEffects[index] })
  }

  function handleEqChange(index: number, value: number) {
    const nextEq = [...eq]
    nextEq[index] = value
    setEq(nextEq)
    updateDirty(effects, nextEq)
    hostCommand('eq-band', { index, value })
  }

  function handleEqReset(index: number) {
    const nextEq = [...eq]
    nextEq[index] = baselineEq[index] ?? 0
    setEq(nextEq)
    updateDirty(effects, nextEq)
    hostCommand('eq-band', { index, value: nextEq[index] })
  }

  function handleEqResetAll() {
    const nextEq = [...baselineEq]
    setEq(nextEq)
    updateDirty(effects, nextEq)
    hostCommand('eq-reset', { values: nextEq })
  }

  function handleSavePreset(name: string) {
    if (hasHost()) {
      hostCommand('save-preset', { name })
    } else {
      const preset: Preset = {
        id: `custom-${Date.now()}`,
        name,
        group: 'custom',
        effects: [...effects],
        eq: [...eq],
      }
      setPreviewCustomPresets((current) => [...current, preset])
      setPresetId(preset.id)
      setBaselineEffects([...effects])
      setBaselineEq([...eq])
      setDirty(false)
      showToast(`Saved “${name}”`)
    }
    setSettingsOpen(false)
    setSaveOpen(false)
  }

  function handleImport() {
    if (!hasHost()) {
      showToast('Preset import is available in the desktop app.')
      return
    }
    hostCommand('import-presets')
    showToast('Choose one or more .fac files.')
  }

  function handleExport() {
    if (!hasHost()) {
      showToast('Preset export is available in the desktop app.')
      return
    }
    hostCommand('export-preset', { id: presetId })
    showToast('Choose where to export the preset.')
  }

  function handleAutostartChange(enabled: boolean) {
    setAutostart(enabled)
    hostCommand('autostart', { enabled })
  }

  function handleThemeChange(nextTheme: ThemeMode) {
    setTheme(nextTheme)
    hostCommand('theme', { theme: nextTheme })
  }

  function handleAccentChange(nextAccent: string) {
    setAccentHex(nextAccent)
    hostCommand('accent', { accent: nextAccent })
  }

  function handleMinimize() {
    hostCommand('minimize')
  }

  function handleClose() {
    hostCommand('close')
  }

  function handleQuit() {
    setSettingsOpen(false)
    const handled = hostCommand('quit')
    if (!handled) showToast('Quit is handled by the desktop shell.')
  }

  function handleDrag() {
    hostCommand('drag')
  }

  const suggestion = useMemo(() => {
    const base = `${selectedPreset.name} Custom`
    if (!presetOptions.some((preset) => preset.name === base)) return base
    let counter = 2
    while (presetOptions.some((preset) => preset.name === `${base} ${counter}`)) {
      counter += 1
    }
    return `${base} ${counter}`
  }, [presetOptions, selectedPreset.name])

  return (
    <div className="app-shell" data-power={power ? 'on' : 'off'}>
      <AppHeader
        power={power}
        onPowerChange={handlePowerChange}
        onOpenSettings={() => setSettingsOpen(true)}
        onMinimize={handleMinimize}
        onClose={handleClose}
        onDrag={handleDrag}
      />

      <main className="workspace" data-power={power ? 'on' : 'off'}>
        <section className="selector-row" aria-label="Routing and presets">
          <PresetSelect
            presets={presetOptions}
            value={presetId}
            dirty={dirty}
            disabled={!power}
            onChange={handlePresetChange}
          />
          <OutputSelect options={OUTPUTS} value={output} onChange={handleOutputChange} />
        </section>

        <Spectrum power={power} accentRgb={accentRgb} />

        <div className="panel-row">
          <EffectRack
            values={effects}
            disabled={!power}
            onChange={handleEffectChange}
            onReset={handleEffectReset}
          />
          <Equalizer
            values={eq}
            disabled={!power}
            onChange={handleEqChange}
            onReset={handleEqReset}
            onResetAll={handleEqResetAll}
          />
        </div>
      </main>

      <SettingsDialog
        open={settingsOpen}
        onOpenChange={setSettingsOpen}
        theme={theme}
        onThemeChange={handleThemeChange}
        accentId={accentId}
        accentHex={accentHex}
        onAccentChange={handleAccentChange}
        autostart={autostart}
        onAutostartChange={handleAutostartChange}
        onImport={handleImport}
        onExport={handleExport}
        onSavePreset={() => setSaveOpen(true)}
        onQuit={handleQuit}
      />

      <SavePresetDialog
        open={saveOpen}
        onOpenChange={setSaveOpen}
        suggestion={suggestion}
        existingNames={presetOptions.map((preset) => preset.name)}
        onSave={handleSavePreset}
      />

      <Toast message={toast?.text ?? null} />
    </div>
  )
}

export default App
