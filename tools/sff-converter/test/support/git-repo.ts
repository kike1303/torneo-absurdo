// A throwaway git repository with its own .gitignore (002 RF-B5). The GIT_* variables of the
// pre-push hook are removed, so nothing here ever touches the real repository.
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import path from 'node:path';
import { cleanGitEnv } from '../../src/write.ts';

export interface TempRepo {
  readonly root: string;
  cleanup(): void;
}

export function tempDir(prefix = 'sff-'): string {
  return realpathSync(mkdtempSync(path.join(tmpdir(), prefix)));
}

export function tempRepo(gitignore = 'ignored/\n'): TempRepo {
  const root = tempDir('sff-repo-');
  const result = spawnSync('git', ['init', '-q'], {
    cwd: root,
    env: cleanGitEnv(process.env),
    stdio: 'ignore',
  });
  if (result.status !== 0) throw new Error('git init failed');
  writeFileSync(path.join(root, '.gitignore'), gitignore);
  mkdirSync(path.join(root, 'tracked'));
  return { root, cleanup: () => rmSync(root, { recursive: true, force: true }) };
}
