import { readFileSync, readdirSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { describe, expect, it } from 'vitest';

// 012 RF43 and plan-c C6: the web does no arithmetic on coins. Every amount, cap, estimate and net
// comes from the server; only `presentation.ts` turns the pool into the share of a bar, `payout_bp`
// into a multiplier and a `net` into what was lost.
const SRC = fileURLToPath(new URL('../src/', import.meta.url));
const DIRS = [path.join('lib', 'features', 'bets'), path.join('lib', 'components', 'bets')];
const ALLOWED = path.join('lib', 'components', 'bets', 'presentation.ts');
const ROUTE = path.join('routes', '+page.svelte');

const FIELDS =
  'stake|staked|max_stake|balance|open_stakes|payout|payout_bp|net|amount|min_bet|fight_cap|bonus_amount|bailout_amount|rake|residue|rake_bp';
const OPERATOR = String.raw`(?:[-+*/%](?![=>/*])|\*\*)`;
const AFTER = new RegExp(String.raw`\.(?:${FIELDS})\b\s*${OPERATOR}`);
const BEFORE = new RegExp(String.raw`${OPERATOR}\s*[\w$]+(?:\??\.[\w$]+)*\??\.(?:${FIELDS})\b`);

function sources(dir: string): string[] {
  return readdirSync(dir, { withFileTypes: true }).flatMap((entry) => {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) return sources(full);
    return /\.(?:ts|svelte)$/.test(entry.name) && !/\.test\.ts$|fixtures\.ts$/.test(entry.name)
      ? [full]
      : [];
  });
}

/** The code without comments, strings or markup text, line by line. */
function code(text: string): string[] {
  return text
    .replace(/\/\*[\s\S]*?\*\//g, '')
    .replace(/<!--[\s\S]*?-->/g, '')
    .split('\n')
    .map((line) => line.replace(/\/\/.*$/, '').replace(/'[^']*'|"[^"]*"/g, "''"));
}

function moneyMath(text: string): string[] {
  return code(text).filter((line) => AFTER.test(line) || BEFORE.test(line));
}

describe('no arithmetic on coins (RF43)', () => {
  it('finds what it is meant to find', () => {
    expect(moneyMath('const x = wallet.balance - mine.staked;')).toHaveLength(1);
    expect(moneyMath('coins(-result.net)')).toHaveLength(1);
    expect(moneyMath('a + b.payout')).toHaveLength(1);
    expect(moneyMath('if (wallet.balance < wallet.min_bet) return;')).toEqual([]);
    expect(moneyMath('const id = `bet-${fight.id}`; // a.stake - 1')).toEqual([]);
    expect(moneyMath('(x) => mine.staked > 0')).toEqual([]);
  });

  it('only presentation.ts does it, in the panel and in the home page', () => {
    const files = [...DIRS.flatMap((dir) => sources(path.join(SRC, dir))), path.join(SRC, ROUTE)];
    expect(files.some((file) => file.endsWith(ALLOWED))).toBe(true);
    const found = files
      .filter((file) => !file.endsWith(ALLOWED))
      .flatMap((file) =>
        moneyMath(readFileSync(file, 'utf8')).map((line) => `${path.relative(SRC, file)}: ${line}`),
      );
    expect(found).toEqual([]);
  });
});
