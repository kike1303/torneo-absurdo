import { expect, test } from '@playwright/test';
import { EGRESS_LOG, expectEgress, fileLines } from './support/logs.js';

// 008 RF-D7 ("nothing leaves loopback"), plan-d decision 10 and change 1 of architect: the
// browser reaches the outside only through the egress proxy, which records the host and cuts
// it, also when the outside is the destination of a redirect (which `page.route` never sees).
const TARGET = 'example.com:443';

async function newEgress(before: number): Promise<string[]> {
  await expect.poll(() => fileLines(EGRESS_LOG).length).toBeGreaterThan(before);
  return fileLines(EGRESS_LOG).slice(before);
}

test.beforeEach(() => expectEgress(TARGET));

test('a goto to another origin is recorded and cut', async ({ page }) => {
  const before = fileLines(EGRESS_LOG).length;
  await expect(page.goto('https://example.com/')).rejects.toThrow();
  expect(await newEgress(before)).toContain(TARGET);
});

test('a 303 from loopback to another origin is recorded and cut', async ({ page }) => {
  const before = fileLines(EGRESS_LOG).length;
  await expect(page.goto('/__e2e/redirect-out')).rejects.toThrow();
  expect(await newEgress(before)).toContain(TARGET);
});
