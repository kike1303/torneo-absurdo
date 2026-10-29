// The `sprites` link of `content/characters/<slug>/character.json` (007 RF-B6). Only `sprites`
// is read: every game rule is crates/content's and the engine's. Directories only; loose files
// (the README) and links are left to `load_roster`, as it does.
import path from 'node:path';
import { LIMITS } from '@torneo/renderer/manifest';
import { LINK_CODES as L } from './codes.ts';
import { kindAt, listDir, readBounded } from './fsx.ts';
import { TOOL_LIMITS } from './limits.ts';
import { relFile, type IssueSink } from './report.ts';
import { SETS_DIR } from './set.ts';

export const CHARACTERS_DIR = ['content', 'characters'] as const;
const CHARACTER_FILE = 'character.json';
const MISPLACED = 'sprites';

export interface LinkCounts {
  readonly characters: number;
  readonly links: number;
}

function readCharacter(file: string): Record<string, unknown> | null {
  const read = readBounded(file, TOOL_LIMITS.jsonFileMaxBytes);
  if (!read.ok) return null;
  let json: unknown;
  try {
    json = JSON.parse(read.bytes.toString('utf8'));
  } catch {
    return null;
  }
  if (typeof json !== 'object' || json === null || Array.isArray(json)) return null;
  return json as Record<string, unknown>;
}

export function checkLinks(sink: IssueSink, root: string): LinkCounts {
  const dir = path.join(root, ...CHARACTERS_DIR);
  if (kindAt(dir) !== 'dir') return { characters: 0, links: 0 };
  let characters = 0;
  let links = 0;
  for (const entry of listDir(dir)) {
    if (entry.kind !== 'dir') continue;
    characters++;
    const rel = relFile(...CHARACTERS_DIR, entry.name);
    if (kindAt(path.join(dir, entry.name, MISPLACED)) !== undefined) {
      sink.add(L.misplacedSprites, relFile(rel, MISPLACED), '');
    }
    const file = relFile(rel, CHARACTER_FILE);
    const json = readCharacter(path.join(dir, entry.name, CHARACTER_FILE));
    if (json === null) {
      sink.add(L.characterUnreadable, file, '');
      continue;
    }
    if (!Object.hasOwn(json, 'sprites')) continue;
    const id = json['sprites'];
    if (typeof id !== 'string' || !LIMITS.idPattern.test(id)) {
      sink.add(L.invalid, file, '/sprites');
    } else if (kindAt(path.join(root, ...SETS_DIR, id)) !== 'dir') {
      sink.add(L.unknownSet, file, '/sprites', { set: id });
    } else {
      links++;
    }
  }
  return { characters, links };
}
