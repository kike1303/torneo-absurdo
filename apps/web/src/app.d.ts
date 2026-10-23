import type { ThemeChoice } from '#lib/theme/resolve-theme.js';

// See https://svelte.dev/docs/kit/types#app.d.ts
declare global {
  namespace App {
    // Kit 3 already declares `status: number` and `message: string` (plan-a decision 4).
    interface Error {
      errorId?: string;
    }
    interface Locals {
      theme: ThemeChoice;
    }
  }
}

export {};
