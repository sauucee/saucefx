interface ToastProps {
  message: string | null
}

export function Toast({ message }: ToastProps) {
  return (
    <div className="toast-region" aria-live="polite" aria-atomic="true">
      {message ? (
        <div className="toast" role="status" key={message}>
          {message}
        </div>
      ) : null}
    </div>
  )
}
