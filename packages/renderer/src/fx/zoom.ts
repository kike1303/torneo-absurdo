// Punch zoom on `special` impacts and on the KO (RF-A12): ≤ ZOOM_PEAK for ≤ ZOOM_MS.

export const ZOOM_MS = 300;
export const ZOOM_PEAK = 1.15;

export function punchZoom(tau: number): number {
  if (!(tau > 0 && tau < ZOOM_MS)) return 1;
  return 1 + (ZOOM_PEAK - 1) * Math.sin((Math.PI * tau) / ZOOM_MS);
}
