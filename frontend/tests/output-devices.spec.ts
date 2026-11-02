import { expect, test, type Page } from '@playwright/test'
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
  theme: 'dark', accent: '#f2486f', customAccent: false, version: '0.2.3',
}

test.beforeEach(async ({ page }) => {
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
})

async function emitOutputs(page: Page, state: NativeOutputs) {
  await page.evaluate((value) => {
    (window as unknown as { outputTest: { emit(state: NativeOutputs): void } }).outputTest.emit(value)
  }, state)
}

test('live device changes preserve sound edits and select the actual headset', async ({ page }) => {
  const output = page.getByRole('combobox', { name: 'Output', exact: true })
  await expect(output).toContainText('Automatic')
  await expect(output).toContainText('Using Speakers · switches automatically')
  const clarity = page.getByRole('slider', { name: 'Clarity level', exact: true })
  const eq = page.getByRole('slider', { name: '1 kHz band', exact: true })
  await clarity.press('ArrowRight')
  await eq.press('ArrowUp')
  await expect(page.getByText('Edited', { exact: true })).toBeVisible()
  await expect(clarity).toHaveValue('1')
  await expect(eq).toHaveValue('1')

  const bluetooth: OutputOption = {
    id: 'bluetooth-headset', label: 'Analog Output — Wireless headset Analog Stereo', detail: 'Processed by SauceFX', available: true,
  }
  const plugged = [...outputs, bluetooth].map((item) => item.id === 'headphones'
    ? { ...item, available: true, detail: 'Processed by SauceFX' } : item)
  const emit = (next: NativeOutputs) => emitOutputs(page, next)
  await emit({ output: 'saucefx', outputs: plugged })
  await output.click()
  await expect(page.getByRole('option', { name: /^Headphones Built-in headphone jack/ })).toBeEnabled()
  await page.getByRole('option', { name: /^Wireless headset/ }).click()
  await expect(output).toContainText('Wireless headset')
  const lastCommand = await page.evaluate(() =>
    (window as unknown as { outputTest: { commands: unknown[] } }).outputTest.commands.at(-1))
  expect(lastCommand).toEqual({ type: 'output', payload: { id: 'bluetooth-headset' } })

  await emit({
    output: 'bluetooth-headset',
    outputs: [...outputs, { ...bluetooth, available: false, detail: 'Disconnected · Line Out — Built-in Audio' }],
  })
  await expect(output).toContainText('Not connected · using Speakers')
  await output.click()
  await expect(page.getByRole('option', { name: /^Wireless headset/ })).toBeDisabled()
  await expect(page.getByRole('option', { name: /^Headphones Plug in wired headphones/ })).toBeDisabled()
  await page.keyboard.press('Escape')
  await emit({ output: 'bluetooth-headset', outputs: plugged })
  await expect(output).toContainText('Audio output')
  await expect(page.getByText('Edited', { exact: true })).toBeVisible()
  await expect(clarity).toHaveValue('1')
  await expect(eq).toHaveValue('1')
})

test('device names stay familiar while USB, Bluetooth and display routes remain distinct', async ({ page }, testInfo) => {
  const deviceId = (name: string, port = '') =>
    `device:${Buffer.from(name).toString('base64')}:${Buffer.from(port).toString('base64')}`
  const usb: OutputOption = {
    id: deviceId('alsa_output.usb-Example_Office_Headset-00.analog-stereo', 'analog-output'),
    label: 'Analog Output — Office Headset Analog Stereo', detail: 'Processed by SauceFX', available: true,
  }
  const bluetooth: OutputOption = {
    id: deviceId('bluez_output.00_11_22_33_44_55.1'),
    label: 'Studio Headphones', detail: 'Processed by SauceFX', available: true,
  }
  const display: OutputOption = {
    id: deviceId('alsa_output.pci-0000_01_00.1.hdmi-stereo', 'hdmi-output-0'),
    label: 'HDMI / DisplayPort — Monitor Audio Digital Stereo (HDMI)',
    detail: 'Processed by SauceFX', available: true,
  }
  await emitOutputs(page, { output: 'saucefx', outputs: [...outputs, usb, bluetooth, display] })
  const output = page.getByRole('combobox', { name: 'Output', exact: true })
  await output.click()
  await expect(page.getByRole('group', { name: 'With SauceFX effects' })).toBeVisible()
  await expect(page.getByRole('group', { name: 'Not connected' })).toBeVisible()
  await expect(page.getByRole('group', { name: 'Without effects' })).toBeVisible()
  await expect(page.getByRole('option', { name: /^Speakers Built-in audio/ })).toBeEnabled()
  await expect(page.getByRole('option', { name: /^Office Headset USB audio/ })).toBeEnabled()
  await expect(page.getByRole('option', { name: /^Studio Headphones Bluetooth audio/ })).toBeEnabled()
  await expect(page.getByRole('option', { name: /^HDMI \/ DisplayPort Monitor Audio/ })).toBeEnabled()
  await expect(page.getByRole('option', { name: /^Direct playback Use system output without SauceFX/ })).toBeEnabled()
  await expect(page.getByRole('listbox')).not.toContainText('Analog Stereo')
  await page.screenshot({ path: testInfo.outputPath('output-menu.png') })
  await page.getByRole('option', { name: /^Office Headset USB audio/ }).click()
  await expect(output).toContainText('Office Headset')
  const lastCommand = await page.evaluate(() =>
    (window as unknown as { outputTest: { commands: unknown[] } }).outputTest.commands.at(-1))
  expect(lastCommand).toEqual({ type: 'output', payload: { id: usb.id } })

  const unavailable = {
    ...usb, label: 'alsa_output.usb-Example_Office_Headset-00.analog-stereo',
    detail: 'Disconnected · Line Out — Built-in Audio', available: false,
  }
  await emitOutputs(page, { output: usb.id, outputs: [...outputs, unavailable] })
  await expect(output).toContainText('Saved USB device')
  await expect(output).toContainText('Not connected · using Speakers')
  await expect(output).not.toContainText('alsa_output')
  await emitOutputs(page, {
    output: usb.id,
    outputs: [...outputs, { ...unavailable, detail: 'Disconnected · Waiting for an audio device' }],
  })
  await expect(output).toContainText('no other audio device available')
})
