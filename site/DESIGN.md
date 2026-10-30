# DESIGN

One-page marketing site for a desktop audio app. The visual world is a **rack
unit**: near-black metal, hairline rules, mono readouts, one hot accent that
moves like a VU needle. Not a SaaS card grid.

## Source of truth

| What | Where |
| --- | --- |
| Colour, type, spacing, motion tokens | `src/styles/theme.css` |
| Base + component styles | `src/styles/global.css` |
| Product copy and commands | `src/content/site.ts` |
| Theme state, accent list | `src/theme/ThemeProvider.tsx` |

No component hard-codes a colour. If you need a new shade, add a token.

## Colour

Two themes on `<html data-theme>`, six accents on `<html data-accent>`.

| Token | Dark | Soft light | Role |
| --- | --- | --- | --- |
| `--bg` | `#0a0a0b` | `#e9e5dd` | page |
| `--bg-deep` | `#050506` | `#ded9cf` | code blocks, wells |
| `--panel` | `#111113` | `#f6f2ea` | cards, panels |
| `--ink` | `#f4f1ec` | `#141210` | primary text |
| `--ink-soft` | `#cec9c1` | `#3b3630` | body copy |
| `--muted` | `#928c83` | `#625b52` | labels, metadata |
| `--line` | 15% white | 17% black | rules, borders |
| `--accent` | palette | palette | fills, meters |
| `--accent-text` | `= --accent` | accent mixed 44% into `#1c0a10` | icons, strokes |

`--accent-text` exists because a vivid pink is fine as a fill on near-black but
fails AA as a glyph on paper. Every foreground use of the accent goes through
it; every fill stays on `--accent`. It clears 4.5:1 against `--bg` for all six
accents in the light theme.

Accents (hex declared once in `:root` as `--palette-*`): Pulse `#ff2e57`,
Amber `#ffb020`, Lime `#c2f03c`, Mint `#2ee6a8`, Cyan `#38c6ff`, Violet
`#b08cff`. `--accent-ink` is the text colour for accent fills and is dark in
every accent, which is why buttons read as buttons.

## Type

Figtree Variable, self-hosted (`public/fonts`, OFL 1.1). One family; contrast
comes from weight and tracking, not a second face.

- Display: 860–880 weight, `-0.05em` tracking, `0.82` line-height.
- Body: 400–560 weight, `1.55` line-height, max `62ch`.
- Technical readouts: system mono stack, uppercase, `0.08em` tracking, 0.68rem.
  Used for data (Hz, dB, step counts), never as decorative eyebrows.

Scale is fluid: `--step--2` through `--step-5`, plus `--step-display`
(`clamp(3.4rem, 18vw, 17rem)`) for the wordmark.

## Layout

- Shell: max `1560px`, gutters `clamp(1.1rem, 4.2vw, 4.25rem)`.
- Full-bleed rules separate bands; sections breathe at
  `clamp(4.75rem, 10.5vw, 10.5rem)`.
- Hero: above 900px the wordmark is lifted out of flow and anchored near the top
  of the stage, so the app window covers its lower half and reads *in front*.
  Below 900px the two stack — a fully covered wordmark reads as a bug, and the
  depth gag needs room.
- Radius: 2 / 4px. Controls and panels are near-square; the type and rules carry
  the character, not rounded cards.

## Motion

- One entrance: the curtain lifts, then the hero lifts once. Sections do not
  each fade in.
- Scroll: Lenis drives ScrollTrigger; they share one raf loop.
- Reduced motion: no curtain, no pin, no cursor tilt, no drag momentum, no
  marquee animation. Static state must still say everything.
- Durations: 140ms state, 260ms theme, 0.55–1.25s scene changes.

## Section map

1. Curtain — EQ bars + 00→100, clip-path wipe. Once per session.
2. Hero — SAUCEFX behind a tilting, draggable app window, spectrum floor.
3. Stats — 5 / 31 / 6 / 2, counted on entry.
4. Marquee — the five effect names, pauses on hover/focus.
5. The rack — pinned, one effect per scroll step, five distinct response plots.
6. EQ playground — 31 draggable bands, live curve, five demo shapes.
7. Feature rail — draggable cards, native scroll + arrows.
8. Install — Flatpak / RPM tabs, copy, requirements disclosure.
9. Footer — closing line, licence, attribution.

## Asset note

The brief referenced `assets/Images/saucefx-logo.svg` and
`docs/images/saucefx-main.png`. The authentic files now live in
`app components/` (`saucefx-1.svg`, `saucefx-main.png`).

- `src/assets/saucefx-logo.svg` is the real app mark, with the export's C2PA
  metadata stripped. The same file is `public/favicon.svg`, and
  `public/apple-touch-icon.png` and the social card's mark are rendered from it.
- The hero window is the **authentic capture** (`src/assets/saucefx-main.png`),
  rendered by `AppScreenshot.tsx` inside the tilting/dragging frame. Intrinsic
  width and height are set on the image, so the frame reserves its space before
  the PNG decodes. On phones the labels become texture, but the window shape,
  spectrum and curve still read as the real app.
- Because it is a real capture, the window keeps its own pink accent when the
  site theme or accent changes. That is deliberate: a screenshot that recolours
  itself would no longer be a screenshot. The page surface, controls and
  spectrum follow the tokens.

