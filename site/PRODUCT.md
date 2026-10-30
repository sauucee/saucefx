# PRODUCT

## What this is

The marketing site for **SauceFX**, a Linux sound EQ and audio enhancement app
based on FxSound. One page, static, no backend.

## Audience

Linux desktop users who care about their audio chain: PipeWire tinkerers, Fedora
and Flatpak users, people who have used FxSound on Windows and want the DSP
engine without the platform. They judge software by what it runs on, not by
badges, so the page leads with the interface and install commands.

## Job of the page

Convince a technical visitor in under thirty seconds that SauceFX is a real
native app — not a wrapper, not a mockup — and get them to a working install
command. Secondary: link out to the source and the release artefacts.

## Positioning

- The FxSound DSP engine, running natively on Linux.
- A framed, opinionated desktop app: 5-effect rack, 31-band graphic EQ with live
  spectrum, PipeWire virtual sink with automatic routing.
- Independent and open source (AGPL-3.0-or-later), not affiliated with FxSound.

## Constraints

- Every product claim traces to the repository `README.md`. No invented
  features, no invented numbers, no invented press.
- Static build, GitHub Pages compatible (relative asset paths), also fine on
  Vercel.
- Vite + React + TypeScript, GSAP + ScrollTrigger, Lenis, Tailwind, Lucide.
- Figtree only. Dark by default; soft light theme is opt-in.
- Motion is ambience: `prefers-reduced-motion` removes the curtain, pinning and
  drag momentum rather than merely shortening them.

## Stack notes

- `src/content/site.ts` is the single source of product copy and command
  strings. Editing claims should happen there.
- `src/styles/theme.css` is the single source of colour and type tokens.
- `scripts/verify.mjs` drives the built site in a real browser (see README).
