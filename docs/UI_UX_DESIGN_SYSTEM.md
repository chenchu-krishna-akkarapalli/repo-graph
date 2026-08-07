# Repo Graph UI/UX Design System

> Agent reference distilled from the 15 local Apple Human Interface Guidelines copies in `docs/guidlines/`. This is an implementation index, not a replacement for the source text. For a task, retrieve this file’s relevant section and then the named source file’s matching section.

## How agents use this reference

### Retrieval rule

1. Classify the request: accessibility, colour/theme, layout, materials, type, components, motion, assets, inclusion/accounts, windows, or immersive UI.
2. Retrieve only the matching section below and the source file(s) listed in its traceability row. Do not ingest all 15 files for a one-component change.
3. Read the actual target component and its states before editing. This reference supplies design intent; it does not supply the target code.
4. Return the change with: intent, source rules applied, tokens/components changed, responsive states, accessibility checks, and remaining risks.

### Seven-piece context stack for UI work

| Piece | What the agent supplies |
|---|---|
| Instructions | This document, the relevant source HIG file, repo rules, and non-negotiable accessibility constraints. |
| User input | The exact screen/component, desired outcome, platform, and explicit constraints. |
| Retrieved facts | Target file contents, current tokens, component states, and only the relevant HIG excerpts. |
| Tools | The permitted read/edit/render/test tools; never execute application code during static design analysis. |
| Short-term notes | Current plan, files read, decisions, open risks, and verification results. |
| Long-term memory | Stable Repo Graph tokens, component conventions, platform assumptions, and approved exceptions. |
| Output format | Intent → changes → states → accessibility → responsive behaviour → verification. |

### Four context-management actions

- **Write:** put plans, source files read, decisions, and open questions in the runtime scratchpad.
- **Select:** retrieve the smallest high-signal source and code slices for the current UI task.
- **Compress:** replace completed investigation with a short decision and evidence note; retain unresolved risks only.
- **Isolate:** keep asset, layout, accessibility, motion, and content reviews distinct when they have different evidence or owners.

## Non-negotiable HIG quality bar

- Content and task completion outrank decoration or brand exposure.
- Prefer familiar platform patterns, semantic controls, progressive disclosure, and recognition over recall.
- Every interactive component has purpose, anatomy, states (default/hover/focus/active/disabled/loading/error/empty), responsive behaviour, and an accessible name.
- Adapt to appearance, text size, locale/RTL, window size, input method, safe areas, and accessibility settings.
- Never communicate essential meaning through colour, sound, motion, translucency, or a gesture alone.
- Do not invent Apple platform values when the source gives a system-defined API or behaviour; use semantic project tokens for web implementation.

## 1. Colour and theme

### Source rules

- Use one colour consistently for one meaning. Do not use an interactive accent on unrelated noninteractive text.
- Prefer dynamic/system semantic colours. If a custom colour is required, provide light, dark, and increased-contrast variants; do not hard-code a documented system RGB value.
- Validate colour in light, dark, increased-contrast, different lighting, displays, and colour profiles. Supply sRGB/P3 asset variants when similar colours or gradients lose fidelity.
- Never use colour as the only state or focus cue: add label, icon, shape, border, weight, or motion. Avoid culturally ambiguous status colours without a textual cue.
- Use colour sparingly on Liquid Glass. Reserve strong colour for primary actions/status; keep toolbars and tab bars monochromatic over busy imagery.
- On glass, colour the primary-action background rather than every symbol/label. Use bold text or large areas for colour; avoid low-contrast lightweight text.

### Repo Graph token contract

```css
:root {
  --canvas: #f7f8fb; --surface: rgb(255 255 255 / .78); --surface-raised: #fff;
  --text: #172033; --text-muted: #526078; --border: rgb(23 32 51 / .12);
  --accent: #2563eb; --success: #15803d; --warning: #b45309; --danger: #dc2626; --info: #0369a1;
}
.dark {
  --canvas: #080d18; --surface: rgb(17 24 39 / .78); --surface-raised: #182235;
  --text: #f8fafc; --text-muted: #b6c2d4; --border: rgb(255 255 255 / .10);
  --accent: #60a5fa; --success: #4ade80; --warning: #fbbf24; --danger: #fb7185; --info: #38bdf8;
}
```

Target contrast: 4.5:1 normal text, 3:1 large text/non-text indicators, and a visible focus ring with 3:1 contrast. Verify actual rendered surfaces, not token values in isolation.

## 2. Layout, grid, and safe areas

- Put the primary content in reading order; use proximity, alignment, negative space, separators, and surfaces to group related items. Do not crowd unrelated controls.
- Let backgrounds and scrollable content extend to window edges while controls float above content. Respect safe areas and system-provided margins/guides.
- Use flexible layout primitives; adapt to orientation, display/window resize, external displays, Dynamic Type, localization, and RTL. Test the smallest and largest supported layouts first.
- Use progressive disclosure for long dependency/import/export lists. Keep the graph primary and put deep inspection in a sidebar/drawer.
- Repo Graph baseline: 4px grid; `gap-3` within a component, `gap-4` between related controls, `gap-6` between sections; cards/panels use `rounded-xl` and `p-4`/`p-5`.

```tsx
<main className="min-h-dvh bg-[var(--canvas)] text-[var(--text)]">
  <div className="grid min-h-dvh grid-cols-1 lg:grid-cols-[17rem_minmax(0,1fr)_22rem]">
    <aside className="p-4 lg:p-5">…</aside><section className="min-w-0">…</section><aside className="p-4 lg:p-5">…</aside>
  </div>
</main>
```

## 3. Materials, Liquid Glass, and depth

- Materials separate controls/text from content and establish hierarchy; they are not decorative blur.
- Use regular/thicker material for text-heavy alerts, sidebars, popovers, and busy backgrounds. Use clear material only over visually rich content that remains legible.
- Choose material by semantic use case, never by the colour it happens to impart. Use blur, vibrancy, blending, and a scrim only when they improve structure/legibility.
- Put vibrant semantic colours on materials. Limit custom glass to important functional elements; stacking multiple translucent controls obscures the content beneath.
- Preserve default system glass when the platform provides it. Provide a more opaque/high-contrast fallback when transparency is reduced.

```html
<aside class="rounded-xl border border-white/10 bg-[var(--surface)] backdrop-blur-xl shadow-[0_12px_32px_rgb(0_0_0_/_0.18)]">…</aside>
```

Depth order: canvas → persistent panel → popover/dropdown → modal/toast. Do not use arbitrary glows, bevels, or blur-on-blur.

## 4. Typography and hierarchy

- Use readable sizes and weights at every viewing distance. Prefer a small, coherent type family set; custom fonts must remain legible and support bold/larger text.
- Use system text styles where available. For Repo Graph, use `Inter, ui-sans-serif, system-ui` for UI and `JetBrains Mono, ui-monospace` for paths/code/symbols.
- Hierarchy comes from role, size, weight, leading, tracking, placement, and spacing before colour or decoration. Keep primary text strongly contrasted against its actual material.
- Support at least 200% text enlargement (140% for watchOS source guidance); preserve important content, minimise truncation, and stack/reduce columns when large text makes inline layouts collide.
- Never truncate useful scrollable content without a way to open/read the rest. Test long names, translated strings, RTL, and large text.

| Role | Tailwind baseline |
|---|---|
| Page title | `text-2xl font-semibold leading-tight tracking-tight` |
| Section title | `text-lg font-semibold leading-snug` |
| Card/node title | `text-sm font-medium leading-5` |
| Body | `text-sm leading-6` |
| Metadata | `text-xs font-medium uppercase tracking-wide` |
| Path/code | `font-mono text-xs leading-5` |

## 5. Component and interaction standards

### Repo Graph surfaces

- **Toolbar:** neutral glass, grouped related controls, visible sync/indexing status; no colourful control pile over graph content.
- **Canvas:** node minimum 120×40px; drag to pan, scroll/pinch to zoom, Shift-click/box-select for multi-select, double-click to focus/open details, Escape to deselect. Hover/selection highlights the node and connected edges.
- **Sidebar/inspector:** desktop trailing panel; mobile drawer/overlay. Show metadata first, then collapsible dependency links and impact analysis. Keep actions grouped by proximity.
- **Buttons:** semantic `<button>`, min 44×44px icon target, explicit destructive label, visible focus. Primary uses accent background; secondary uses neutral surface.
- **Inputs/search:** visible/programmatic label, `min-h-11`, clear empty/loading/no-result/error states, no icon-only meaning.
- **Tabs/accordions:** semantic tablist and arrow navigation; accordions expose `aria-expanded`/`aria-controls`.
- **Badges/status:** text or icon plus colour; never colour alone. Use `aria-live="polite"` for nonurgent indexing/watcher changes.

### Component output contract

For each component, document purpose, anatomy, states, size/variants, consumed tokens, motion, accessibility, mobile/desktop behaviour, edge cases, and composition rules before implementation.

## 6. Motion and animation

- Motion must explain feedback, causality, hierarchy, status, or instruction. It must be brief, precise, realistic relative to the triggering gesture, optional, and cancellable.
- Repo Graph timing: fast 150ms (hover/focus), base 200ms (state), slow 300ms (drawer/popover). Do not block the next action.
- Avoid frequent gratuitous animation, peripheral motion, z-axis/blur transitions that reduce comfort, sustained oscillation near 0.2Hz, and large moving objects that fill the field of view.
- For relocation with no useful travel meaning, fade out → move → fade in. Keep a stationary frame of reference in immersive contexts.
- Respect `prefers-reduced-motion`; replace decorative animation with immediate state, text, icon, or haptic feedback. Do not make motion the only status channel.

```css
.ui-interactive { transition: color 150ms ease-out, background-color 150ms ease-out, border-color 150ms ease-out, box-shadow 200ms ease-out; }
@media (prefers-reduced-motion: reduce) { *, *::before, *::after { animation-duration: .01ms !important; transition-duration: .01ms !important; scroll-behavior: auto !important; } }
```

## 7. Accessibility and inclusive use

- Meet WCAG AA: normal text 4.5:1, large text 3:1, non-text indicators 3:1; test light/dark/increased contrast. Use system semantic colours where possible.
- Support larger text, Dynamic Type, VoiceOver/screen readers, Voice Control, Full Keyboard Access, Switch Control, Pointer/AssistiveTouch, and alternative input paths.
- Use semantic HTML and landmarks; labelled controls; logical heading order; skip link; `aria-describedby` for errors; `aria-hidden` for decoration; polite live regions for dynamic status.
- Keyboard: Tab/Shift+Tab moves; Enter/Space activates; arrows navigate composite controls; Escape closes modal/popover; never positive `tabindex` or keyboard traps. Modal focus moves in, traps, closes with Escape, and returns to trigger.
- Use simple familiar gestures and provide a visible alternative. Avoid auto-dismiss timers for important information; give people explicit dismissal and playback controls.
- Use about 12pt padding around bezel controls and 24pt around unbezeled visible edges; maintain at least 44×44px targets. Never assume disability limits interest or capability.
- Use plain language, culturally considerate colour/copy, representative imagery, text expansion, RTL, and locale-aware formatting. Do not require an account unless core functionality needs it; minimise data entry and explain required fields.

## 8. Icons and images

- App icons: one recognisable idea, simple filled/overlapping shapes, crisp foreground edges, centred primary content, safe zone, opaque background, and light/dark variants where needed. Avoid nonessential text, static system effects, soft edges, and excessive detail.
- Interface icons: one concept per icon, familiar geometry, optical alignment, consistent weight, accessible name, and contrast at rendered size. Use SF Symbols/system symbols where the platform provides them.
- Images: deliver correct scale factors and colour profiles; preserve focal content when cropping; avoid distortion; use appearance-specific assets if one image loses contrast. Informative images need descriptive alt text; decorative images use empty alt text.
- Text over imagery needs a material/scrim/placement that remains legible; do not rely on shadows. Keep primary content inside icon/image safe zones because scaling, parallax, and masking can crop edges.

## 9. Windows, immersive, and spatial surfaces

- Windows should adapt fluidly to resize and multitasking. Do not open new windows by default; offer them when preserving context or multitasking benefits the user. Prefer system window controls/frames and user-facing term “window.”
- Do not put critical information/actions in a bottom bar that can be hidden; use a trailing inspector for rich detail. Keep window controls clear of toolbar items.
- Choose initial/min/max window size and shape for content; avoid empty space, overlapping controls, and unusable extremes. Preserve system glass and appearance transitions.
- Use a window for familiar UI; use a volume only when rich 3D depth is meaningful. Keep 2D content readable from multiple angles and limit ornaments to one high-value addition without competing with toolbar/tab bar.
- In immersive/spatial UI, minimise distraction, provide grounding/stationary reference, keep content in a comfortable field of view, avoid head-locked content and large repetitive gestures, and offer a clear Shared Space/non-immersive fallback.

## 10. Source traceability and agent retrieval map

| Source file | Retrieve when the task involves | Apply these source-level decisions |
|---|---|---|
| `accessibility.md` | assistive tech, controls, media, motion comfort | Dynamic Type, contrast, labels, keyboard, alternatives to gestures, explicit dismissal, target spacing, spatial comfort |
| `Branding.md` | logo, tone, custom font, brand colour | brand defers to content; standard patterns remain familiar; no logo-heavy launch screen |
| `Color.md` | palettes, status, accents, glass colour | semantic/dynamic variants, no colour-only meaning, lighting/display/profile testing, restrained glass colour |
| `Darkmode.md` | themes, dark surfaces, icons | system appearance, base/elevated backgrounds, ≥4.5:1 minimum and stronger small-text contrast, adaptive assets |
| `Materials.md` | blur, translucency, vibrancy, glass | material by semantic use, regular vs clear, vibrant foreground, contrast/fallback, limited custom glass |
| `typography.md` | type scale, custom fonts, labels | legibility, few families, Dynamic Type, 200% enlargement, low truncation, stacked large-text layouts |
| `layout.md` | grids, responsive screens, safe areas | hierarchy, alignment, progressive disclosure, full-bleed content, guides/safe areas, context adaptation |
| `motions.md` | transitions, feedback, spatial animation | purposeful/optional/brief/precise/cancellable motion, reduced motion, stationary reference, no sustained oscillation |
| `icons.md` | app icon or icon asset | simple concept, safe zones, layers, opaque background, appearance variants, system effects |
| `iconso.md` | interface/document icons | single concept, optical alignment, simple shapes, legible safe margins and document labels |
| `images.md` | images, scale, crop, colour assets | resolution/scale factors, aspect ratio, colour profile, focal content, platform display constraints |
| `inclusions.md` | copy, imagery, localisation, audiences | respectful language, avoid assumptions/stereotypes, broad representation, internationalisation and RTL |
| `managingaccounts.md` | auth, profiles, sign-in, data entry | account only when needed, trusted sign-in/autofill, minimum data, shared profiles, clear privacy states |
| `windows.md` | desktop/iPad/vision windows | adaptive sizing, native controls, focus/toolbar boundaries, glass, window vs volume, ornaments |
| `impressiveexperience.md` | immersive/spatial experiences | Shared vs Full Space, grounding, dim/tint passthrough, attention hierarchy, comfort, avoid redundant assets |

## 11. Verification checklist

- [ ] Relevant source file(s) were read and named in the change notes.
- [ ] Target component was read; no design decision was inferred from the manifest alone.
- [ ] Light/dark, increased contrast, reduced transparency, reduced motion, large text, narrow width, RTL, and long-content states were considered where relevant.
- [ ] Content hierarchy remains primary; branding, glass, colour, image, and motion are purposeful.
- [ ] Keyboard, screen reader, focus, labels, target size, contrast, and non-colour cues are verified.
- [ ] Empty/loading/error/disabled/permission/destructive states are explicit and actionable.
- [ ] The output states intent, source rules applied, code/tokens changed, responsive behaviour, accessibility checks, and remaining risks.

### Local source inventory

This reference covers every local source: `accessibility.md`, `Branding.md`, `Color.md`, `Darkmode.md`, `Materials.md`, `typography.md`, `layout.md`, `motions.md`, `icons.md`, `iconso.md`, `images.md`, `inclusions.md`, `managingaccounts.md`, `windows.md`, and `impressiveexperience.md`.
