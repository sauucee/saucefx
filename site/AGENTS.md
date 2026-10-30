# AGENTS.md — Website

## Current state

Clean slate. This folder contains only:

- `.agents/skills/` — 26 installed skills (25 tracked in the lock file; `impeccable`
  was installed separately)
- `skills-lock.json` — skill sources and hashes
- `opencode.json` — MCP config (`shadcn`)
- this file

No app, build config, assets, or design docs yet. Scaffold the new site
deliberately; do not resurrect the previous SauceFX site.

`threejs-animation` comes from the granular `cloudai-x/threejs-skills` set, which
was judged the better of the two same-named skills: it targets three r186 with
current APIs (`THREE.Timer`, Clock deprecation), and goes deeper on the actual
animation domain (clips, mixers, actions, crossfades, bones, morphs, additive,
TSL). The broader `iart-ai/webgl-animation-skills` version was not installed
because its R3F, instancing, post-processing and disposal material is already
covered by the installed `threejs-react-three-fiber`, `threejs-performance`,
`threejs-postprocessing` and related skills.

## Skill policy

This project uses every skill installed under `.agents/skills/`. They are tracked
in `skills-lock.json`; refresh them with `npx skills update`. Load the skills that
fit the task, and combine them rather than picking one.

| Work | Skills to load |
| --- | --- |
| Any UI change, review, polish, audit, motion, a11y | `impeccable` |
| Setting or reshaping the visual direction | `frontend-design` |
| Scroll-driven sections, layered heroes, scrollytelling | `scroll-craft` |
| GSAP work | `gsap-react` + `gsap-core`, plus `gsap-timeline` / `gsap-scrolltrigger` / `gsap-plugins` as needed |
| GSAP perf, easing, helpers | `gsap-performance`, `gsap-utils` |
| Three.js / R3F work | `threejs-fundamentals` + the relevant `threejs-*` skills |
| Particles, sparks, confetti, flow fields, GPU points | `particle-system` |
| Custom GLSL, SDFs, shader backgrounds, transitions, distortion | `shader-glsl` |
| Pre-ship UI and accessibility review | `web-design-guidelines` |
| Finding or installing another capability | `find-skills` |
| UI primitives | `shadcn` MCP, styled with project tokens |

## Precedence

Once they exist, the project's own source of truth wins over generic skill advice:

1. `DESIGN.md` and `.impeccable/design.json` — tokens, type, colour, spacing.
2. `PRODUCT.md` — audience, positioning, stack constraints.
3. Existing code patterns and the current page architecture.

Skills inform method; they must not replace the project palette, typography, or
locked design direction with their own defaults.

## Working expectations

- Keep the build static and GitHub Pages compatible (relative paths).
- Respect `prefers-reduced-motion`; motion is ambience, not the message.
- Check desktop and mobile, and run the build before calling work done.
- `scroll-craft`'s engine is never edited per project — theme it with tokens.
- Don't double-drive scroll: if a section uses scroll-craft's engine, it owns the
  scroll timeline there; keep GSAP ScrollTrigger to sections that opt into it.
