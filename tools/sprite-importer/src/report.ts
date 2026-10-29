// The output of the CLI (007 RF-B2, plan-b decision 15): one JSON line per issue, keys in the
// order code, file, path, data. `file` is relative (to the repo root, or to the export folder in
// `import`) or empty; `path` is a JSON Pointer of the manifest (or of the JSON file), empty for
// the whole file. Never an absolute path, never text.
import { LIMITS } from '@torneo/renderer/manifest';
import { REPORT_CODES } from './codes.ts';

export type IssueData = Readonly<Record<string, string | number>>;

export interface Issue {
  readonly code: string;
  readonly file: string;
  readonly path: string;
  readonly data?: IssueData;
}

export function issue(code: string, file: string, path: string, data?: IssueData): Issue {
  return data === undefined ? { code, file, path } : { code, file, path, data };
}

/** Thrown when the sink is full; the commands stop there. */
export class TooManyIssues {
  readonly stop = true;
}

/** Collects issues up to `LIMITS.maxErrors`; one more is `report.too_many_errors` and stops. */
export class IssueSink {
  readonly issues: Issue[] = [];

  add(code: string, file: string, path: string, data?: IssueData): void {
    if (this.issues.length >= LIMITS.maxErrors) {
      this.issues.push(issue(REPORT_CODES.tooManyErrors, '', ''));
      throw new TooManyIssues();
    }
    this.issues.push(issue(code, file, path, data));
  }

  get count(): number {
    return this.issues.length;
  }
}

/** One line, keys in a fixed order. */
export function line(value: Issue | Readonly<Record<string, number>>): string {
  if ('code' in value && typeof value.code === 'string') {
    const i = value as Issue;
    return JSON.stringify(
      i.data === undefined
        ? { code: i.code, file: i.file, path: i.path }
        : { code: i.code, file: i.file, path: i.path, data: i.data },
    );
  }
  return JSON.stringify(value);
}

/** Joins path segments with `/`, whatever the platform (for `file`). */
export function relFile(...segments: readonly string[]): string {
  return segments.filter((s) => s !== '').join('/');
}
