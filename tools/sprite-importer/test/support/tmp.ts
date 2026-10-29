// Temporary directories for one test file, removed at the end.
import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';

const made: string[] = [];

/** A fresh directory (its real path, so macOS `/var` → `/private/var` does not matter). */
export function tmp(prefix = 'sprite-importer-'): string {
  const dir = realpathSync(mkdtempSync(path.join(os.tmpdir(), prefix)));
  made.push(dir);
  return dir;
}

export function cleanupTmp(): void {
  for (const dir of made.splice(0)) rmSync(dir, { recursive: true, force: true });
}
