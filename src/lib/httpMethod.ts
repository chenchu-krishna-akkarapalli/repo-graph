/** HTTP method derivation + badge palette for route symbols (§20). */

const METHODS = ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS', 'HEAD'] as const

export const METHOD_ORDER: readonly string[] = METHODS

/**
 * Route symbol names are URL paths; some carry a method token (Beego
 * `[get]`, MapPost-derived names, `POST /x` prefixes). Look for a method
 * substring, defaulting to GET.
 */
export function httpMethodOf(name: string): string {
  const upper = name.toUpperCase()
  for (const m of METHODS) {
    if (upper.includes(m)) return m
  }
  return 'GET'
}

/** Tailwind classes per method badge, tuned for the #0D0F12 dark canvas. */
export const METHOD_BADGE: Record<string, string> = {
  GET: 'border-emerald-500/40 bg-emerald-500/10 text-emerald-300',
  POST: 'border-sky-500/40 bg-sky-500/10 text-sky-300',
  PUT: 'border-amber-500/40 bg-amber-500/10 text-amber-300',
  PATCH: 'border-purple-500/40 bg-purple-500/10 text-purple-300',
  DELETE: 'border-red-500/40 bg-red-500/10 text-red-300',
  OPTIONS: 'border-slate-500/40 bg-slate-500/10 text-slate-300',
  HEAD: 'border-slate-500/40 bg-slate-500/10 text-slate-300',
}
