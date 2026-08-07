/**
 * O(1) hover highlighting for the graph canvas.
 *
 * Dimming 1,000+ nodes through React/Zustand costs one selector run per node
 * per mouse move (~50 ms frame time on large repos). Instead we rewrite a
 * single CSSOM rule per hover: the browser's selector engine does the
 * matching natively on the GPU-composited `opacity` property, so the React
 * tree never re-renders and frame time stays under ~2 ms.
 *
 * Node elements advertise their identity via `data-node-path` and their
 * adjacency via `data-node-neighbors` (a `|a|b|c|` delimited string built
 * once in `buildFlow`). Edges advertise `data-edge-source` /
 * `data-edge-target`.
 */

/** Attribute-safe delimiter for the neighbor set (paths never contain `|`). */
export const NEIGHBOR_DELIM = '|'

/** Wrap a neighbor list so `[data-node-neighbors*="|p|"]` matches exactly. */
export function neighborAttr(neighbors: Iterable<string>): string {
  const parts = [...neighbors]
  if (parts.length === 0) return NEIGHBOR_DELIM
  return NEIGHBOR_DELIM + parts.join(NEIGHBOR_DELIM) + NEIGHBOR_DELIM
}

/** Dim rule for everything on the canvas — inserted once when a hover
 *  begins, removed when it ends. Keeping it resident between moves matters:
 *  inserting/removing a blanket selector invalidates every node's style,
 *  while swapping the narrow `[data-node-path=…]` rules below only touches
 *  the handful of elements that can match them. */
const DIM_RULES = [
  '.canvas-container .graph-node { opacity: 0.25; }',
  '.canvas-container .graph-edge { opacity: 0.12; }',
]

let sheet: CSSStyleSheet | null = null
let lastPath: string | null | undefined = undefined

function styleSheet(): CSSStyleSheet | null {
  if (sheet) return sheet
  if (typeof document === 'undefined') return null
  const el = document.createElement('style')
  el.setAttribute('data-repo-graph', 'hover-highlight')
  document.head.appendChild(el)
  sheet = el.sheet as CSSStyleSheet
  return sheet
}

/** CSS string literal escaping (paths may contain quotes/backslashes). */
function cssString(value: string): string {
  return `"${value.replace(/\\/g, '\\\\').replace(/"/g, '\\"')}"`
}

/**
 * Point the highlight rules at `path` (or clear them). Constant work:
 * one rule delete + one rule insert, regardless of graph size.
 */
export function applyHoverHighlight(path: string | null): void {
  if (path === lastPath) return
  lastPath = path
  const css = styleSheet()
  if (!css) return

  if (path === null) {
    while (css.cssRules.length > 0) css.deleteRule(0)
    return
  }

  // Dim rules stay put across moves; only the highlight rules after them are
  // rewritten, so a mouse sweep never re-invalidates the whole canvas.
  if (css.cssRules.length === 0) {
    for (const rule of DIM_RULES) css.insertRule(rule, css.cssRules.length)
  }
  while (css.cssRules.length > DIM_RULES.length) css.deleteRule(css.cssRules.length - 1)

  const p = cssString(path)
  const neighbor = cssString(NEIGHBOR_DELIM + path + NEIGHBOR_DELIM)

  // Neighbors (and the hovered node itself) stay fully lit; everything else
  // is dimmed by the static `[data-hovered-path]` rule in index.css.
  css.insertRule(
    `[data-node-path=${p}], [data-node-neighbors*=${neighbor}] { opacity: 1 !important; }`,
    css.cssRules.length,
  )
  css.insertRule(
    `[data-node-path=${p}] { z-index: 10; box-shadow: 0 0 0 1px rgba(255,255,255,0.25), 0 12px 32px rgba(0,0,0,0.7) !important; }`,
    css.cssRules.length,
  )
  // Outgoing dependencies gold, incoming dependents red — matches the
  // previous per-edge React logic without re-rendering any edge component.
  // `insertRule` takes exactly one rule per call.
  css.insertRule(
    `g[data-edge-source=${p}], g[data-edge-target=${p}] { opacity: 1 !important; }`,
    css.cssRules.length,
  )
  css.insertRule(
    `g[data-edge-source=${p}] .react-flow__edge-path { stroke: var(--color-impact-yellow) !important; stroke-width: 2 !important; }`,
    css.cssRules.length,
  )
  css.insertRule(
    `g[data-edge-target=${p}] .react-flow__edge-path { stroke: var(--color-impact-red) !important; stroke-width: 2 !important; }`,
    css.cssRules.length,
  )
}
