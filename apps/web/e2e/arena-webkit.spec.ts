import { expect, test } from '@playwright/test';
import { ENGINE_LOGS } from '../src/lib/features/fight/engine-logs.js';

// 005 RF18 (plan decision 11) and 006 (Estrategia de pruebas): WebKit plays the seven goldens to
// the end at the minimum pace (`pace=0` is clamped to it). Chromium covers the rest in
// arena.spec.ts. If this engine has no WebGL (WebKit on Linux), the route reports
// webgl_unavailable and the test is annotated `limited`, like csp.spec.ts: then the check is
// repeated by hand in Safari.
const ending = (name: string) =>
  name.startsWith('ko-') ? 'ko' : name === 'draw' ? 'draw' : 'decision';

for (const name of Object.keys(ENGINE_LOGS)) {
  test(`?log=${name}&pace=0 reaches finished, or webgl_unavailable (limited)`, async ({ page }) => {
    const errors: string[] = [];
    page.on('pageerror', (error) => errors.push(error.message));
    await page.goto(`/es/dev/arena?log=${name}&pace=0&t=99999999999`);
    const stage = page.getByTestId('fight-stage');
    await expect(stage).toHaveAttribute('data-state', /^(finished|error)$/, { timeout: 15_000 });
    if ((await stage.getAttribute('data-state')) === 'error') {
      await expect(stage).toHaveAttribute('data-error', 'webgl_unavailable');
      test.info().annotations.push({ type: 'limited', description: 'no WebGL in this engine' });
    } else {
      await expect(stage).toHaveAttribute('data-ending', ending(name));
    }
    expect(errors).toEqual([]);
  });
}
