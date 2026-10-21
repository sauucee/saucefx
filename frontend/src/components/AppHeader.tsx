import type { PointerEvent } from 'react'
import { Switch } from '@base-ui/react/switch'
import { Minus, Power, Settings2, X } from 'lucide-react'
import { IconButton } from './IconButton'

interface AppHeaderProps {
  power: boolean
  onPowerChange: (power: boolean) => void
  onOpenSettings: () => void
  onMinimize: () => void
  onClose: () => void
  onDrag: () => void
}

export function AppHeader({
  power,
  onPowerChange,
  onOpenSettings,
  onMinimize,
  onClose,
  onDrag,
}: AppHeaderProps) {
  function handlePointerDown(event: PointerEvent<HTMLElement>) {
    if (event.button !== 0) return
    const target = event.target as HTMLElement
    if (target.closest('.no-drag')) return
    onDrag()
  }

  return (
    <header className="titlebar" onPointerDown={handlePointerDown}>
      <div className="brand">
        <img
          className="brand-mark"
          src={`${import.meta.env.BASE_URL}saucefx-mark.svg`}
          alt=""
          draggable={false}
        />
        <div className="brand-copy">
          <span className="brand-name">SauceFX</span>
          <span className="brand-sub">Audio enhancer</span>
        </div>
      </div>

      <div className="titlebar-spacer" />

      <div className="titlebar-actions no-drag">
        <Switch.Root
          className="power-control"
          checked={power}
          onCheckedChange={onPowerChange}
          aria-label="Enable audio processing"
        >
          <Power size={13} strokeWidth={2.2} aria-hidden="true" />
          <span className="power-label">{power ? 'On' : 'Off'}</span>
        </Switch.Root>

        <span className="titlebar-divider" aria-hidden="true" />

        <IconButton label="Settings" onClick={onOpenSettings}>
          <Settings2 size={15} strokeWidth={1.9} aria-hidden="true" />
        </IconButton>
        <IconButton label="Minimize" onClick={onMinimize} className="window-button">
          <Minus size={15} strokeWidth={1.9} aria-hidden="true" />
        </IconButton>
        <IconButton label="Close" onClick={onClose} tone="danger" className="window-button">
          <X size={15} strokeWidth={1.9} aria-hidden="true" />
        </IconButton>
      </div>
    </header>
  )
}
