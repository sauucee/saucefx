import { expect, test } from '@playwright/test'
import type { NativeOutputs, NativeState } from '../src/bridge'
import type { OutputOption } from '../src/data'

const outputs: OutputOption[] = [
  { id: 'system', label: 'System audio', detail: 'Unprocessed system output' },
  { id: 'saucefx', label: 'Automatic (SauceFX)', detail: 'Line Out — Built-in Audio' },
  { id: 'lineout', label: 'Line Out — Built-in Audio', detail: 'Processed by SauceFX', available: true },
  { id: 'headphones', label: 'Headphones — Built-in Audio', detail: 'Not connected', available: false },
]
const initialState: NativeState = {
  power: true, output: 'saucefx', outputs,
  presetId: 'default', presets: [{ id: 'default', name: 'Default', group: 'factory' }],
  effects: [0, 0, 0, 0, 0], eq: Array(31).fill(0), autostart: false,
  theme: 'dark', accent: '#f2486f', customAccent: false, version: '0.2.2',
}

test('live device changes preserve sound edits and select the actual headset', async ({ page }) => {
  await page.addInitScript((state) => {
    const listeners: Record<string, Set<(value: unknown) => void>> = {}
    const subscribe = (name: string) => (listener: (value: unknown) => void) => {
      ;(listeners[name] ??= new Set()).add(listener)
      return () => listeners[name].delete(listener)
    }
    const harness = {
      commands: [] as { type: string; payload: unknown }[],
      outputs: { output: state.output, outputs: state.outputs } as NativeOutputs,
      emit(next: NativeOutputs) {
        this.outputs = next
        listeners.outputs?.forEach((listener) => listener(next))
      },
    }
    Object.assign(window, {
      outputTest: harness,
      saucefxHost: {
        onState: subscribe('state'), onOutputs: subscribe('outputs'),
        onSpectrum: subscribe('spectrum'), onToast: subscribe('toast'),
        command(type: string, payload: { id: string }) {
          harness.commands.push({ type, payload })
          if (type === 'ready')
            queueMicrotask(() => listeners.state?.forEach((listener) => listener(state)))
          if (type === 'output')
            queueMicrotask(() => harness.emit({ ...harness.outputs, output: payload.id }))
        },
      },
    })
  }, initialState)
  await page.goto('/')
  const output = page.getByRole('combobox', { name: 'Output', exact: true })
  await expect(output).toContainText('Automatic (SauceFX)')
  const clarity = page.getByRole('slider', { name: 'Clarity level', exact: true })
  const eq = page.getByRole('slider', { name: '1 kHz band', exact: true })
  await clarity.press('ArrowRight')
  await eq.press('ArrowUp')
  await expect(page.getByText('Edited', { exact: true })).toBeVisible()
  await expect(clarity).toHaveValue('1')
  await expect(eq).toHaveValue('1')

  const bluetooth: OutputOption = {
    id: 'bluetooth-headset', label: 'Wireless headset', detail: 'Processed by SauceFX', available: true,
  }
  const plugged = [...outputs, bluetooth].map((item) => item.id === 'headphones'
    ? { ...item, available: true, detail: 'Processed by SauceFX' } : item)
  const emit = (next: NativeOutputs) => page.evaluate((value) => {
    (window as unknown as { outputTest: { emit(state: NativeOutputs): void } }).outputTest.emit(value)
  }, next)
  await emit({ output: 'saucefx', outputs: plugged })
  await output.click()
  await expect(page.getByRole('option', { name: /^Headphones — Built-in Audio/ })).toBeEnabled()
  await page.getByRole('option', { name: /^Wireless headset/ }).click()
  await expect(output).toContainText('Wireless headset')
  const lastCommand = await page.evaluate(() =>
    (window as unknown as { outputTest: { commands: unknown[] } }).outputTest.commands.at(-1))
  expect(lastCommand).toEqual({ type: 'output', payload: { id: 'bluetooth-headset' } })

  await emit({
    output: 'bluetooth-headset',
    outputs: [...outputs, { ...bluetooth, available: false, detail: 'Disconnected · Line Out — Built-in Audio' }],
  })
  await expect(output).toContainText('Disconnected')
  await output.click()
  await expect(page.getByRole('option', { name: /^Wireless headset/ })).toBeDisabled()
  await expect(page.getByRole('option', { name: /^Headphones — Built-in Audio/ })).toBeDisabled()
  await page.keyboard.press('Escape')
  await emit({ output: 'bluetooth-headset', outputs: plugged })
  await expect(output).toContainText('Processed by SauceFX')
  await expect(page.getByText('Edited', { exact: true })).toBeVisible()
  await expect(clarity).toHaveValue('1')
  await expect(eq).toHaveValue('1')
})
