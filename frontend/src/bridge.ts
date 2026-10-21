import type { PresetOption, ThemeMode } from './data'

export type HostCommand =
  | 'ready'
  | 'power'
  | 'preset'
  | 'output'
  | 'effect'
  | 'eq-band'
  | 'eq-reset'
  | 'save-preset'
  | 'import-presets'
  | 'export-preset'
  | 'theme'
  | 'accent'
  | 'autostart'
  | 'minimize'
  | 'close'
  | 'quit'
  | 'drag'

export interface NativeState {
  power: boolean
  output: string
  presetId: string
  presets: PresetOption[]
  effects: number[]
  eq: number[]
  autostart: boolean
  theme: ThemeMode
  accent: string
  customAccent: boolean
  version: string
}

type Unsubscribe = () => void

interface SauceFXHost {
  command(type: HostCommand, payload?: unknown): void
  onState(listener: (state: NativeState) => void): Unsubscribe
  onSpectrum(listener: (bands: number[]) => void): Unsubscribe
  onToast(listener: (message: string) => void): Unsubscribe
}

declare global {
  interface Window {
    saucefxHost?: SauceFXHost
  }
}

export function hasHost(): boolean {
  return typeof window !== 'undefined' && Boolean(window.saucefxHost)
}

export function hostCommand(type: HostCommand, payload?: unknown): boolean {
  try {
    if (!window.saucefxHost) return false
    window.saucefxHost.command(type, payload)
    return true
  } catch {
    // The browser preview has no native host. State still lives in React,
    // so an unavailable bridge is harmless while the UI is being built.
    return false
  }
}

export function onNativeState(listener: (state: NativeState) => void): Unsubscribe {
  try {
    return window.saucefxHost?.onState(listener) ?? (() => {})
  } catch {
    return () => {}
  }
}

export function onNativeSpectrum(listener: (bands: number[]) => void): Unsubscribe {
  try {
    return window.saucefxHost?.onSpectrum(listener) ?? (() => {})
  } catch {
    return () => {}
  }
}

export function onNativeToast(listener: (message: string) => void): Unsubscribe {
  try {
    return window.saucefxHost?.onToast(listener) ?? (() => {})
  } catch {
    return () => {}
  }
}

/** Reads a value injected by the JUCE host before the page loads. */
export function initialHostValue(name: string): string | null {
  try {
    const data = (
      window as Window & {
        __JUCE__?: { initialisationData?: Record<string, unknown> }
      }
    ).__JUCE__?.initialisationData?.[name]
    return Array.isArray(data) && typeof data[0] === 'string' ? data[0] : null
  } catch {
    return null
  }
}
