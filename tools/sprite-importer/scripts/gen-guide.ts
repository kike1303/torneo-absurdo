// Rewrites the generated block of docs/guides/sprites.md from the real ruleset golden (007 RF-B7,
// plan-b decision 24). Exits with 2 without writing when the golden is invalid or the markers are
// missing or repeated.
import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { loadGolden } from '../src/golden.ts';
import { renderGuideBlock, replaceGuideBlock } from '../src/guide-doc.ts';

const ROOT = fileURLToPath(new URL('../../../', import.meta.url));
const GUIDE = path.join(ROOT, 'docs', 'guides', 'sprites.md');

const rulesets = loadGolden(ROOT);
if (rulesets === null) {
  process.stdout.write(JSON.stringify({ code: 'io.golden_invalid', file: '', path: '' }) + '\n');
  process.exit(2);
}
const next = replaceGuideBlock(readFileSync(GUIDE, 'utf8'), renderGuideBlock(rulesets));
if (next === null) {
  process.stdout.write(
    JSON.stringify({ code: 'cli.guide_markers', file: 'docs/guides/sprites.md', path: '' }) + '\n',
  );
  process.exit(2);
}
writeFileSync(GUIDE, next);
