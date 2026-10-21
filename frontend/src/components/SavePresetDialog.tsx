import { useEffect, useMemo, useState } from 'react'
import { Dialog } from '@base-ui/react/dialog'
import { Field } from '@base-ui/react/field'
import { Input } from '@base-ui/react/input'
import { X } from 'lucide-react'

interface SavePresetDialogProps {
  open: boolean
  onOpenChange: (open: boolean) => void
  suggestion: string
  existingNames: string[]
  onSave: (name: string) => void
}

export function SavePresetDialog({
  open,
  onOpenChange,
  suggestion,
  existingNames,
  onSave,
}: SavePresetDialogProps) {
  const [name, setName] = useState(suggestion)

  useEffect(() => {
    if (open) setName(suggestion)
  }, [open, suggestion])

  const normalized = name.trim()
  const duplicate = useMemo(
    () =>
      existingNames.some(
        (existing) => existing.trim().toLowerCase() === normalized.toLowerCase(),
      ),
    [existingNames, normalized],
  )
  const error =
    normalized.length === 0
      ? 'Enter a name for this preset.'
      : duplicate
        ? 'A preset with this name already exists.'
        : null
  const canSave = error === null

  function handleSave() {
    if (!canSave) return
    onSave(normalized)
    onOpenChange(false)
  }

  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Backdrop className="dialog-backdrop dialog-backdrop-nested" />
        <Dialog.Viewport className="dialog-viewport">
          <Dialog.Popup className="dialog save-dialog">
            <header className="dialog-head">
              <div>
                <Dialog.Title className="dialog-title">Save preset</Dialog.Title>
                <Dialog.Description className="dialog-description">
                  Creates a new custom preset. Factory presets are never overwritten.
                </Dialog.Description>
              </div>
              <Dialog.Close className="icon-button" aria-label="Close save dialog">
                <X size={15} strokeWidth={1.9} aria-hidden="true" />
              </Dialog.Close>
            </header>

            <div className="save-dialog-body">
              <Field.Root invalid={Boolean(error)}>
                <Field.Label className="field-label">Preset name</Field.Label>
                <Input
                  className="text-input"
                  value={name}
                  onValueChange={setName}
                  autoFocus
                  onKeyDown={(event) => {
                    if (event.key === 'Enter') {
                      event.preventDefault()
                      handleSave()
                    }
                  }}
                />
                {error ? (
                  <p className="field-error" role="alert">
                    {error}
                  </p>
                ) : (
                  <p className="field-help">
                    The current effect and EQ values will be saved with this name.
                  </p>
                )}
              </Field.Root>
            </div>

            <footer className="save-dialog-footer">
              <Dialog.Close className="button button-secondary">Cancel</Dialog.Close>
              <button
                type="button"
                className="button button-primary"
                onClick={handleSave}
                disabled={!canSave}
              >
                Save preset
              </button>
            </footer>
          </Dialog.Popup>
        </Dialog.Viewport>
      </Dialog.Portal>
    </Dialog.Root>
  )
}
