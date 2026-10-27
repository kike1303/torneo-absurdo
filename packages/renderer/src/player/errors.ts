// Errors of the renderer handle (RF-A11, RF-A16). Codes, never text (ADR-0007).

export const RENDERER_ERROR_CODES = [
  'webgl_unavailable',
  'asset_invalid',
  'asset_load_failed',
  'timeline_invalid',
] as const;
export type RendererErrorCode = (typeof RENDERER_ERROR_CODES)[number];

export class RendererError extends Error {
  readonly code: RendererErrorCode;

  constructor(code: RendererErrorCode, options?: { cause?: unknown }) {
    super(code, options);
    this.name = 'RendererError';
    this.code = code;
  }
}

export function isRendererError(value: unknown): value is RendererError {
  return value instanceof RendererError;
}
