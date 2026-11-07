import { REPLAY_ROWS, seedReplays } from './db.mjs';
import { addDynamicCanary } from './logs.js';

// The `globalSetup` of playwright.replays.config.ts: the server already migrated, because Playwright
// starts `webServer` before this (011 "Estrategia de pruebas"). The seeds and fragments of the logs
// are canaries of the teardown: public by now, and still never in a log (ADR-0012 A4).
export default async function seed(): Promise<void> {
  const { ids, secrets } = await seedReplays();
  // replays.spec.ts asks for them by position, so a database that is not new fails here.
  ids.forEach((id, index) => {
    if (id !== index + 1) throw new Error(`seed: ${REPLAY_ROWS[index]?.name} got id ${id}`);
  });
  for (const secret of new Set(secrets)) addDynamicCanary(secret);
}
