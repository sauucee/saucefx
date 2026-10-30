import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';

export type ThemeMode = 'dark' | 'light';
export type AccentId = 'pulse' | 'amber' | 'lime' | 'mint' | 'cyan' | 'violet';

/** The six presets the app ships with. Colours live in `styles/theme.css`. */
export const ACCENTS: ReadonlyArray<{ id: AccentId; label: string }> = [
  { id: 'pulse', label: 'Pulse' },
  { id: 'amber', label: 'Amber' },
  { id: 'lime', label: 'Lime' },
  { id: 'mint', label: 'Mint' },
  { id: 'cyan', label: 'Cyan' },
  { id: 'violet', label: 'Violet' },
];

export const STORAGE_KEY = 'saucefx-site:appearance';

type Appearance = { theme: ThemeMode; accent: AccentId };

type ThemeContextValue = Appearance & {
  setTheme: (theme: ThemeMode) => void;
  setAccent: (accent: AccentId) => void;
  toggleTheme: () => void;
};

const ThemeContext = createContext<ThemeContextValue | null>(null);

function readStored(): Appearance | null {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as Partial<Appearance>;
    const theme = parsed.theme === 'light' || parsed.theme === 'dark' ? parsed.theme : null;
    const accent = ACCENTS.some((a) => a.id === parsed.accent) ? (parsed.accent as AccentId) : null;
    return theme && accent ? { theme, accent } : null;
  } catch {
    return null;
  }
}

function initial(): Appearance {
  // Dark is the brand default. Soft light is a deliberate opt-in that mirrors
  // the app's second theme, so the OS preference is not consulted.
  return readStored() ?? { theme: 'dark', accent: 'pulse' };
}

export function ThemeProvider({ children }: { children: ReactNode }) {
  const [appearance, setAppearance] = useState<Appearance>(initial);

  useEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = appearance.theme;
    root.dataset.accent = appearance.accent;
    const meta = document.querySelector('meta[name="theme-color"]');
    // Read the page token, not a literal: the browser chrome colour always
    // matches whatever `theme.css` says `--bg` is for the active theme.
    const bg = getComputedStyle(root).getPropertyValue('--bg').trim();
    if (meta && bg) {
      meta.setAttribute('content', bg);
    }
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(appearance));
    } catch {
      /* private mode — the choice just won't persist */
    }
  }, [appearance]);

  const setTheme = useCallback((theme: ThemeMode) => {
    setAppearance((prev) => (prev.theme === theme ? prev : { ...prev, theme }));
  }, []);

  const setAccent = useCallback((accent: AccentId) => {
    setAppearance((prev) => (prev.accent === accent ? prev : { ...prev, accent }));
  }, []);

  const toggleTheme = useCallback(() => {
    setAppearance((prev) => ({ ...prev, theme: prev.theme === 'dark' ? 'light' : 'dark' }));
  }, []);

  const value = useMemo<ThemeContextValue>(
    () => ({ ...appearance, setTheme, setAccent, toggleTheme }),
    [appearance, setTheme, setAccent, toggleTheme],
  );

  return <ThemeContext.Provider value={value}>{children}</ThemeContext.Provider>;
}

export function useTheme(): ThemeContextValue {
  const ctx = useContext(ThemeContext);
  if (!ctx) throw new Error('useTheme must be used inside <ThemeProvider>');
  return ctx;
}
