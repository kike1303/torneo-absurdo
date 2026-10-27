// Writes packages/renderer/placeholders/<id>/{manifest.json, sheet-0.png} (RF-A6).
// Run: pnpm --filter @torneo/renderer gen:placeholders. The freshness test compares pixels and
// JSON with what drawPlaceholder() returns, so the output is committed.
import { mkdirSync, writeFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { PNG } from 'pngjs';
import { PLACEHOLDER_IDS, drawPlaceholder } from './placeholders/draw.ts';

const root = fileURLToPath(new URL('../placeholders/', import.meta.url));

for (const id of PLACEHOLDER_IDS) {
  const { manifest, sheets } = drawPlaceholder(id);
  const dir = `${root}${id}/`;
  mkdirSync(dir, { recursive: true });
  writeFileSync(`${dir}manifest.json`, JSON.stringify(manifest, null, 2) + '\n');
  for (const sheet of sheets) {
    const png = new PNG({ width: sheet.width, height: sheet.height });
    png.data.set(sheet.data);
    writeFileSync(`${dir}${sheet.file}`, PNG.sync.write(png, { colorType: 6 }));
  }
}
