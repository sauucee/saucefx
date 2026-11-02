export type ThemeMode = 'dark' | 'light'

export type AccentId =
  | 'rose'
  | 'blue'
  | 'green'
  | 'amber'
  | 'violet'
  | 'teal'
  | 'custom'

export interface Accent {
  id: Exclude<AccentId, 'custom'>
  label: string
  value: string
}

export const ACCENTS: Accent[] = [
  { id: 'rose', label: 'Rose', value: '#f2486f' },
  { id: 'blue', label: 'Blue', value: '#3b82f6' },
  { id: 'green', label: 'Green', value: '#2fa572' },
  { id: 'teal', label: 'Teal', value: '#12a8a0' },
  { id: 'amber', label: 'Amber', value: '#d4922a' },
  { id: 'violet', label: 'Violet', value: '#8b7cf6' },
]

export interface EffectDefinition {
  id: string
  name: string
  description: string
}

export const EFFECTS: EffectDefinition[] = [
  { id: 'clarity', name: 'Clarity', description: 'Restores detail and definition' },
  { id: 'ambience', name: 'Ambience', description: 'Adds a sense of space' },
  { id: 'surround', name: 'Surround', description: 'Widens the stereo field' },
  { id: 'dynamic', name: 'Dynamic Boost', description: 'Evens out quiet passages' },
  { id: 'bass', name: 'Bass Boost', description: 'Adds low-end weight' },
]

export const EQ_FREQUENCIES = [
  20, 25, 31.5, 40, 50, 63, 80, 100, 125, 160, 200, 250, 315, 400, 500,
  630, 800, 1000, 1250, 1600, 2000, 2500, 3150, 4000, 5000, 6300, 8000,
  10000, 12500, 16000, 20000,
]

export const EQ_MAX = 12

export const EQ_LABEL_INDICES = [0, 4, 7, 10, 13, 16, 18, 21, 24, 27, 30]

export function formatFrequency(frequency: number, compact = false): string {
  if (frequency >= 1000) {
    const value = frequency / 1000
    const digits = Number.isInteger(value) ? 0 : 1
    return `${value.toFixed(digits)}${compact ? 'k' : ' kHz'}`
  }
  return `${frequency}${compact ? '' : ' Hz'}`
}

export function formatGain(value: number): string {
  /* Preset EQ values are floats (1.200000047...), so keep the hover
   * readout to the same one-decimal precision as the native UI. */
  const rounded = Math.round(value * 10) / 10
  if (rounded === 0) return '0'
  return `${rounded > 0 ? '+' : ''}${rounded.toFixed(1).replace(/\.0$/, '')}`
}

export interface PresetOption {
  id: string
  name: string
  group: 'factory' | 'custom'
}

export interface Preset extends PresetOption {
  effects: number[]
  eq: number[]
}

function curve(anchors: Array<[number, number]>): number[] {
  const output = new Array<number>(EQ_FREQUENCIES.length).fill(0)
  for (let a = 0; a < anchors.length - 1; a += 1) {
    const [startIndex, startValue] = anchors[a]
    const [endIndex, endValue] = anchors[a + 1]
    const span = Math.max(1, endIndex - startIndex)
    for (let index = startIndex; index <= endIndex; index += 1) {
      const t = (index - startIndex) / span
      output[index] = Math.round(startValue + (endValue - startValue) * t)
    }
  }
  return output.map((value) => Math.max(-EQ_MAX, Math.min(EQ_MAX, value)))
}

const flat = curve([
  [0, 0],
  [30, 0],
])

function factoryPreset(
  id: string,
  name: string,
  effects: number[],
  anchors: Array<[number, number]>,
): Preset {
  return { id, name, group: 'factory', effects, eq: curve(anchors) }
}

export const FACTORY_PRESETS: Preset[] = [
  factoryPreset('default', 'Default', [0, 0, 0, 0, 0], [
    [0, 0],
    [30, 0],
  ]),
  factoryPreset('general', 'General', [3, 4, 4, 3, 4], [
    [0, 1],
    [4, 2],
    [8, 1],
    [13, 0],
    [18, 1],
    [23, 2],
    [27, 2],
    [30, 1],
  ]),
  factoryPreset('music', 'Music', [5, 6, 4, 3, 6], [
    [0, 4],
    [5, 3],
    [9, 1],
    [14, 0],
    [18, 1],
    [22, 2],
    [26, 3],
    [30, 3],
  ]),
  factoryPreset('movies', 'Movies', [4, 5, 7, 5, 6], [
    [0, 6],
    [5, 4],
    [11, 1],
    [15, 0],
    [19, 1],
    [23, 3],
    [27, 2],
    [30, 1],
  ]),
  factoryPreset('voice', 'Voice', [7, 2, 1, 4, 1], [
    [0, -6],
    [4, -4],
    [8, 0],
    [12, 3],
    [17, 4],
    [21, 3],
    [25, 1],
    [30, -1],
  ]),
  factoryPreset('volume', 'Volume Boost', [3, 4, 4, 7, 7], [
    [0, 5],
    [4, 4],
    [10, 1],
    [15, -1],
    [20, 0],
    [25, 2],
    [30, 4],
  ]),
  factoryPreset('gaming', 'Gaming', [4, 3, 6, 6, 5], [
    [0, 5],
    [5, 3],
    [10, 0],
    [15, -1],
    [20, 2],
    [24, 4],
    [28, 3],
    [30, 2],
  ]),
  factoryPreset('classic', 'Classic Processing', [4, 5, 5, 3, 4], [
    [0, 3],
    [5, 2],
    [11, 1],
    [16, 2],
    [21, 3],
    [26, 2],
    [30, 3],
  ]),
  factoryPreset('light', 'Light Processing', [2, 2, 2, 2, 2], [
    [0, 1],
    [8, 1],
    [15, 1],
    [23, 1],
    [30, 1],
  ]),
  factoryPreset('bass', 'Bass Boost', [2, 3, 5, 4, 9], [
    [0, 9],
    [4, 7],
    [9, 4],
    [14, 1],
    [20, 0],
    [25, -1],
    [30, 0],
  ]),
  factoryPreset('streaming', 'Streaming Video', [4, 4, 5, 4, 5], [
    [0, 4],
    [5, 3],
    [10, 0],
    [15, -1],
    [19, 2],
    [23, 4],
    [27, 2],
    [30, 1],
  ]),
  factoryPreset('tv', 'TV', [5, 5, 4, 5, 4], [
    [0, 5],
    [5, 4],
    [10, 1],
    [15, -2],
    [20, 1],
    [24, 3],
    [28, 2],
    [30, 1],
  ]),
  factoryPreset('transcription', 'Transcription', [8, 1, 0, 2, 0], [
    [0, -7],
    [4, -5],
    [8, -2],
    [12, 2],
    [17, 4],
    [21, 4],
    [25, 1],
    [30, -2],
  ]),
]

export const SYSTEM_PRESET: Preset = {
  id: 'flat-system',
  name: 'Flat',
  group: 'factory',
  effects: [0, 0, 0, 0, 0],
  eq: flat,
}

export interface OutputOption {
  id: string
  label: string
  detail: string
  available?: boolean
}

export const OUTPUTS: OutputOption[] = [
  {
    id: 'system',
    label: 'System audio',
    detail: 'Unprocessed system output',
  },
  {
    id: 'saucefx',
    label: 'Automatic (SauceFX)',
    detail: 'Follows connected audio devices',
  },
]

export const THEME_STORAGE_KEY = 'saucefx.theme'
export const ACCENT_STORAGE_KEY = 'saucefx.accent'
export const CUSTOM_PRESETS_STORAGE_KEY = 'saucefx.custom-presets'

export function getAccent(id: AccentId): Accent | undefined {
  return ACCENTS.find((accent) => accent.id === id)
}

export function accentIdFor(value: string): AccentId {
  const match = ACCENTS.find(
    (accent) => accent.value.toLowerCase() === value.toLowerCase(),
  )
  return match ? match.id : 'custom'
}
