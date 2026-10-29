// `check` (007 RF-B6, plan-b decision 14): every set of content/sprites/ and every link of
// content/characters/, in byte order of the names. A set without a character and a shared set
// are fine; zero sets pass.
import path from 'node:path';
import { IO_CODES, SET_CODES } from './codes.ts';
import { kindAt, listDir } from './fsx.ts';
import { loadGolden } from './golden.ts';
import { checkLinks } from './link.ts';
import { IssueSink, TooManyIssues, issue, relFile, type Issue } from './report.ts';
import { SETS_DIR, checkSet } from './set.ts';

/** The only loose file accepted in content/sprites/. */
const README = 'README.md';

export interface CheckSummary {
  readonly sets: number;
  readonly characters: number;
  readonly links: number;
}

export interface CheckResult {
  readonly status: 0 | 1 | 2;
  readonly issues: readonly Issue[];
  readonly summary?: CheckSummary;
}

export function runCheck(options: { readonly root: string }): CheckResult {
  const { root } = options;
  const rulesets = loadGolden(root);
  if (rulesets === null) {
    return { status: 2, issues: [issue(IO_CODES.goldenInvalid, '', '')] };
  }
  const sink = new IssueSink();
  let sets = 0;
  let counts = { characters: 0, links: 0 };
  try {
    const dir = path.join(root, ...SETS_DIR);
    const kind = kindAt(dir);
    if (kind !== undefined && kind !== 'dir') {
      sink.add(SET_CODES.unexpectedEntry, relFile(...SETS_DIR), '');
    } else if (kind === 'dir') {
      for (const entry of listDir(dir)) {
        if (entry.kind === 'dir') {
          sets++;
          checkSet(sink, root, entry.name, rulesets);
        } else if (!(entry.kind === 'file' && entry.name === README)) {
          sink.add(SET_CODES.unexpectedEntry, relFile(...SETS_DIR, entry.name), '');
        }
      }
    }
    counts = checkLinks(sink, root);
  } catch (error) {
    if (!(error instanceof TooManyIssues)) throw error;
  }
  if (sink.count > 0) return { status: 1, issues: sink.issues };
  return { status: 0, issues: [], summary: { sets, ...counts } };
}
