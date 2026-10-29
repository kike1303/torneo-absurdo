// Writes the reference export `ref_a` to fixtures/export-ref/ (007 RF-B8, plan-b decision 22),
// removing whatever was there. The output is committed; test/fixture.test.ts fails when it no
// longer matches what scripts/ref/draw.ts draws.
import { mkdirSync, rmSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { PNG } from 'pngjs';
import { drawRef } from './ref/draw.ts';

const OUT = fileURLToPath(new URL('../fixtures/export-ref/', import.meta.url));

const ref = drawRef();
rmSync(OUT, { recursive: true, force: true });
mkdirSync(OUT, { recursive: true });
writeFileSync(path.join(OUT, 'export.json'), JSON.stringify(ref.exportJson, null, 2) + '\n');
for (const [key, image] of ref.frames) {
  const file = path.join(OUT, 'frames', ...`${key}.png`.split('/'));
  mkdirSync(path.dirname(file), { recursive: true });
  const png = new PNG({ width: image.width, height: image.height });
  png.data.set(image.data);
  writeFileSync(file, PNG.sync.write(png, { colorType: 6 }));
}
