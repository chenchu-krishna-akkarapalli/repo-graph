/**
 * Persistence for the CEPA startup-guide opt-out.
 *
 * Split out of `CEPAUserGuideModal.tsx` so that file exports only its
 * component: mixing component and non-component exports in one module defeats
 * React Fast Refresh, which then reloads the whole page on every edit.
 */

export const CEPA_DISMISSED_KEY = 'repograph_cepa_dismissed'

/** True when the user has opted out of the startup guide. */
export function isCepaDismissed(): boolean {
  try {
    return localStorage.getItem(CEPA_DISMISSED_KEY) === 'true'
  } catch {
    // Private-browsing modes throw on localStorage access; showing the guide
    // is the safer default.
    return false
  }
}

export function setCepaDismissed(dismissed: boolean): void {
  try {
    localStorage.setItem(CEPA_DISMISSED_KEY, String(dismissed))
  } catch {
    // Opting out is a convenience, not something worth failing the UI over.
  }
}
