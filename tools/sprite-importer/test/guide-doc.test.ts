import { readFileSync } from 'node:fs';
import path from 'node:path';
import { describe, expect, it } from 'vitest';
import { GUIDE_CODES } from '@torneo/renderer/guide';
import { EXPORT_CODES, LINK_CODES, PROVENANCE_CODES, SET_CODES } from '../src/codes.ts';
import { loadGolden } from '../src/golden.ts';
import {
  GUIDE_END,
  GUIDE_START,
  extractGuideBlock,
  renderGuideBlock,
  replaceGuideBlock,
} from '../src/guide-doc.ts';
import { REPO } from './support/root.ts';

// 007 RF-B7 and plan-b decisions 16 and 24: the committed block is what the generator writes,
// the prose has no time figures and the code table covers every content code.
const TEXT = readFileSync(path.join(REPO, 'docs', 'guides', 'sprites.md'), 'utf8');
const RULESETS = (() => {
  const r = loadGolden(REPO);
  if (r === null) throw new Error('golden');
  return r;
})();
const outside = (text: string) =>
  text.slice(0, text.indexOf(GUIDE_START)) + text.slice(text.indexOf(GUIDE_END));

describe('docs/guides/sprites.md', () => {
  it('has the block the generator writes', () => {
    expect(extractGuideBlock(TEXT), 'run pnpm --filter @torneo/sprite-importer gen:guide').toBe(
      renderGuideBlock(RULESETS),
    );
  });

  it('has each marker once', () => {
    expect(TEXT.split(GUIDE_START)).toHaveLength(2);
    expect(TEXT.split(GUIDE_END)).toHaveLength(2);
  });

  it('writes no time figure outside the block (the prose names the source)', () => {
    expect(outside(TEXT).match(/\b\d+\s?ms\b/g) ?? []).toEqual([]);
  });

  it('is a guide in its frontmatter', () => {
    const front = TEXT.split('---')[1] ?? '';
    expect(front).toMatch(/^type: guide$/m);
    expect(front).toMatch(/^reviewers: \[game-designer, architect\]$/m);
  });

  it('explains every content code outside the block', () => {
    const codes = [
      ...Object.values(EXPORT_CODES),
      ...Object.values(SET_CODES),
      ...Object.values(PROVENANCE_CODES),
      ...Object.values(LINK_CODES),
      ...Object.values(GUIDE_CODES),
    ];
    const prose = outside(TEXT);
    expect(codes.filter((c) => !prose.includes(`\`${c}\``))).toEqual([]);
  });
});

describe('the documented import command', () => {
  // `pnpm import` is a pnpm built-in (it converts lockfiles): without `run`, pnpm never reaches
  // the script.
  it.each(['docs/guides/sprites.md', 'tools/sprite-importer/README.md'])(
    '%s uses run import',
    (file) => {
      const text = readFileSync(path.join(REPO, ...file.split('/')), 'utf8');
      expect(text).toContain('@torneo/sprite-importer run import --');
      expect(text).not.toMatch(/@torneo\/sprite-importer import\b/);
    },
  );
});

describe('renderGuideBlock', () => {
  it('follows the data: a version with a lower heavy windup changes its row and names it', () => {
    const v1 = RULESETS[0];
    if (v1 === undefined) throw new Error('v1');
    const block = renderGuideBlock([v1, { ...v1, ruleset_version: 2, windup_heavy_ms: 280 }]);
    const row = block.split('\n').find((l) => l.startsWith('| `attack_heavy`'));
    expect(row).toContain('250–280 ms (tope del ruleset v2)');
    expect(renderGuideBlock(RULESETS)).not.toContain('tope del ruleset');
  });

  it('replaces only between the markers and refuses missing or repeated ones', () => {
    const doc = `a\n${GUIDE_START}\nold\n${GUIDE_END}\nb\n`;
    expect(replaceGuideBlock(doc, 'new')).toBe(`a\n${GUIDE_START}\nnew\n${GUIDE_END}\nb\n`);
    expect(replaceGuideBlock('no markers', 'x')).toBeNull();
    expect(replaceGuideBlock(doc + GUIDE_START, 'x')).toBeNull();
    expect(replaceGuideBlock(`${GUIDE_END}${GUIDE_START}`, 'x')).toBeNull();
  });
});
