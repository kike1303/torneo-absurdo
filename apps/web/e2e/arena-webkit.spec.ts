import { expect, test } from '@playwright/test';

// 005 RF18 (plan decision 11): WebKit plays the draw golden to the end. Chromium covers the rest
// in arena.spec.ts. If this engine has no WebGL (WebKit on Linux), the route reports
// webgl_unavailable and the test is annotated `limited`, like csp.spec.ts: then the check is
// repeated by hand in Safari.
test('?log=draw reaches finished with the draw, or webgl_unavailable (limited)', async ({
  page,
}) => {
  const errors: string[] = [];
  page.on('pageerror', (error) => errors.push(error.message));
  await page.goto('/es/dev/arena?log=draw&t=99999999999');
  const stage = page.getByTestId('fight-stage');
  await expect(stage).toHaveAttribute('data-state', /^(finished|error)$/, { timeout: 15_000 });
  if ((await stage.getAttribute('data-state')) === 'error') {
    await expect(stage).toHaveAttribute('data-error', 'webgl_unavailable');
    test.info().annotations.push({ type: 'limited', description: 'no WebGL in this engine' });
  } else {
    await expect(stage).toHaveAttribute('data-ending', 'draw');
    await expect(page.getByTestId('fight-draw')).toHaveText('Empate');
  }
  expect(errors).toEqual([]);
});
