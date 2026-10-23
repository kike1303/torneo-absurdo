// See https://svelte.dev/docs/kit/types#app.d.ts
declare global {
  namespace App {
    interface Error {
      errorId?: string;
    }
  }
}

export {};
