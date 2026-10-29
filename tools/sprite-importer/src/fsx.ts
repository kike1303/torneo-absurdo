// Bounded reads that never follow a symlink (007 RF-B2, RF-B3, plan-b decision 11).
import {
  closeSync,
  constants,
  fstatSync,
  lstatSync,
  openSync,
  readSync,
  readdirSync,
} from 'node:fs';
import path from 'node:path';

export type EntryKind = 'file' | 'dir' | 'symlink' | 'other';

export interface Entry {
  readonly name: string;
  readonly kind: EntryKind;
}

function kindOf(stat: { isFile(): boolean; isDirectory(): boolean; isSymbolicLink(): boolean }) {
  if (stat.isSymbolicLink()) return 'symlink';
  if (stat.isFile()) return 'file';
  if (stat.isDirectory()) return 'dir';
  return 'other';
}

/** Compares names by their UTF-8 bytes, like `load_roster` (the same order on every system). */
export function byBytes(a: string, b: string): number {
  return Buffer.compare(Buffer.from(a, 'utf8'), Buffer.from(b, 'utf8'));
}

/** The kind of `file` without following it, or `undefined` when it does not exist. */
export function kindAt(file: string): EntryKind | undefined {
  const stat = lstatSync(file, { throwIfNoEntry: false });
  return stat === undefined ? undefined : kindOf(stat);
}

/** The entries of `dir`, each one with `lstat`, in byte order of their names. */
export function listDir(dir: string): Entry[] {
  return readdirSync(dir)
    .sort(byBytes)
    .map((name) => ({ name, kind: kindAt(path.join(dir, name)) ?? 'other' }));
}

export type ReadResult =
  | { readonly ok: true; readonly bytes: Buffer }
  | {
      readonly ok: false;
      readonly reason: 'missing' | 'symlink' | 'not_file' | 'too_large' | 'changed';
    };

const NOFOLLOW = constants.O_NOFOLLOW ?? 0;

/**
 * Reads at most `max + 1` bytes of a regular file: one more than `max` is `too_large`, whatever
 * the metadata says. The file opened must be the one `lstat` saw (same device and inode), so a
 * swap for a link or another file between the check and the read is `changed`. Other I/O errors
 * (EACCES…) are thrown.
 */
export function readBounded(file: string, max: number): ReadResult {
  const before = lstatSync(file, { throwIfNoEntry: false });
  if (before === undefined) return { ok: false, reason: 'missing' };
  if (before.isSymbolicLink()) return { ok: false, reason: 'symlink' };
  if (!before.isFile()) return { ok: false, reason: 'not_file' };
  let fd: number;
  try {
    fd = openSync(file, constants.O_RDONLY | NOFOLLOW);
  } catch (error) {
    const code = (error as { code?: unknown } | null)?.code;
    if (code === 'ELOOP') return { ok: false, reason: 'symlink' };
    if (code === 'ENOENT') return { ok: false, reason: 'changed' };
    throw error;
  }
  try {
    const after = fstatSync(fd);
    if (!after.isFile() || after.dev !== before.dev || after.ino !== before.ino) {
      return { ok: false, reason: 'changed' };
    }
    const buffer = Buffer.alloc(Math.min(max + 1, Math.max(after.size, 0) + 1));
    let length = 0;
    for (;;) {
      if (length === buffer.length) break;
      const n = readSync(fd, buffer, length, buffer.length - length, null);
      if (n === 0) break;
      length += n;
    }
    // The file may have grown after fstat: one more read tells.
    if (length === buffer.length && length <= max) {
      const probe = Buffer.alloc(1);
      if (readSync(fd, probe, 0, 1, null) > 0) return { ok: false, reason: 'too_large' };
    }
    if (length > max) return { ok: false, reason: 'too_large' };
    return { ok: true, bytes: buffer.subarray(0, length) };
  } finally {
    closeSync(fd);
  }
}
