import { StrictMode } from 'react'
import { createRoot } from 'react-dom/client'
import { Tooltip } from '@base-ui/react/tooltip'
import App from './App'
import './styles.css'

/* The native host is a desktop window, not a browser tab. Keep WebKit's
 * page context menu and accidental file navigation out of the interface. */
document.addEventListener('contextmenu', (event) => event.preventDefault())
document.addEventListener('dragover', (event) => event.preventDefault())
document.addEventListener('drop', (event) => event.preventDefault())

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <Tooltip.Provider delay={450} closeDelay={80}>
      <App />
    </Tooltip.Provider>
  </StrictMode>,
)
