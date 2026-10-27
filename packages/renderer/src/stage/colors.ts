// Scene color roles to theme colors (plan-a2 decision 7): no literals here.
import type { RendererTheme } from '../player/types.ts';
import type { ColorRole } from '../scene/types.ts';

export function roleColor(theme: RendererTheme, role: ColorRole): number {
  return theme.colors[role];
}
