import type { ReactNode } from 'react'
import { Tooltip } from '@base-ui/react/tooltip'

interface IconButtonProps {
  label: string
  children: ReactNode
  onClick?: () => void
  disabled?: boolean
  tone?: 'default' | 'danger'
  className?: string
}

export function IconButton({
  label,
  children,
  onClick,
  disabled,
  tone = 'default',
  className = '',
}: IconButtonProps) {
  return (
    <Tooltip.Root>
      <Tooltip.Trigger
        type="button"
        className={`icon-button ${tone === 'danger' ? 'icon-button-danger' : ''} ${className}`.trim()}
        aria-label={label}
        onClick={onClick}
        disabled={disabled}
      >
        {children}
      </Tooltip.Trigger>
      <Tooltip.Portal>
        <Tooltip.Positioner sideOffset={8}>
          <Tooltip.Popup className="tooltip">{label}</Tooltip.Popup>
        </Tooltip.Positioner>
      </Tooltip.Portal>
    </Tooltip.Root>
  )
}
