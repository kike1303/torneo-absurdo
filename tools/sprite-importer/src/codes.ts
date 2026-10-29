// Codes of the sprite importer and of the set check (007 RF-B1 to RF-B6, plan-b decision 16).
// Codes, never text (ADR-0007): docs/guides/sprites.md is their dictionary. The CLI exits with 1
// on a content code (export, manifest, guide, set, provenance, link, report) and with 2 on a
// usage, I/O or internal one.

/** The open export, `torneo.sprites.export` v1 (RF-B1). */
export const EXPORT_CODES = Object.freeze({
  unexpectedEntry: 'export.unexpected_entry',
  symlink: 'export.symlink',
  missingFile: 'export.missing_file',
  fileTooLarge: 'export.file_too_large',
  invalidJson: 'export.invalid_json',
  unknownField: 'export.unknown_field',
  missingField: 'export.missing_field',
  unsupportedFormat: 'export.unsupported_format',
  invalidValue: 'export.invalid_value',
  unknownAnimation: 'export.unknown_animation',
  frameMissing: 'export.frame_missing',
  frameExtra: 'export.frame_extra',
  pngInvalid: 'export.png_invalid',
  pngSize: 'export.png_size',
  tooManyFrames: 'export.too_many_frames',
} as const);

/** A committed set, `content/sprites/<id>/` (RF-B3). */
export const SET_CODES = Object.freeze({
  unexpectedEntry: 'set.unexpected_entry',
  missingFile: 'set.missing_file',
  fileTooLarge: 'set.file_too_large',
  invalidJson: 'set.invalid_json',
  sheetHeader: 'set.sheet_header',
  sheetInvalid: 'set.sheet_invalid',
} as const);

/** `provenance.json`, `torneo.provenance` v1 (RF-B4). */
export const PROVENANCE_CODES = Object.freeze({
  fileTooLarge: 'provenance.file_too_large',
  invalidJson: 'provenance.invalid_json',
  unknownField: 'provenance.unknown_field',
  missingField: 'provenance.missing_field',
  invalidValue: 'provenance.invalid_value',
  setMismatch: 'provenance.set_mismatch',
} as const);

/** The `sprites` link of `content/characters/<slug>/character.json` (RF-B6). */
export const LINK_CODES = Object.freeze({
  invalid: 'link.invalid',
  unknownSet: 'link.unknown_set',
  misplacedSprites: 'link.misplaced_sprites',
  characterUnreadable: 'link.character_unreadable',
} as const);

/** The cut of the output at `LIMITS.maxErrors` lines. */
export const REPORT_CODES = Object.freeze({ tooManyErrors: 'report.too_many_errors' } as const);

export const CLI_CODES = Object.freeze({
  badArguments: 'cli.bad_arguments',
  unknownCommand: 'cli.unknown_command',
  missingArgument: 'cli.missing_argument',
  inNotADirectory: 'cli.in_not_a_directory',
  outForbidden: 'cli.out_forbidden',
  outIdMismatch: 'cli.out_id_mismatch',
  outSymlink: 'cli.out_symlink',
  outNotADirectory: 'cli.out_not_a_directory',
  outUnexpectedEntry: 'cli.out_unexpected_entry',
} as const);

export const IO_CODES = Object.freeze({
  readFailed: 'io.read_failed',
  writeFailed: 'io.write_failed',
  goldenInvalid: 'io.golden_invalid',
} as const);

export const INTERNAL_CODES = Object.freeze({ unexpected: 'internal.unexpected' } as const);

/** Usage, I/O and internal codes: exit 2. Everything else is content: exit 1. */
export function isUsageCode(code: string): boolean {
  return code.startsWith('cli.') || code.startsWith('io.') || code.startsWith('internal.');
}
