import { describe, expect, it } from 'vitest';
import { IssueSink } from '../src/report.ts';
import { reportReadFailure } from '../src/set.ts';

// QA nit on PR #52: a file that changes between the listing and the read never passes in silence.
describe('reportReadFailure', () => {
  it.each(['missing', 'symlink', 'not_file', 'too_large', 'changed'] as const)(
    'reports %s for a file the listing saw as a regular file',
    (reason) => {
      const sink = new IssueSink();
      reportReadFailure(sink, 'f', '', reason, 'file', 'set.file_too_large');
      expect(sink.issues).toHaveLength(1);
    },
  );

  it('leaves to the listing an entry it already reports as unexpected', () => {
    const sink = new IssueSink();
    reportReadFailure(sink, 'f', '', 'symlink', 'symlink', 'set.file_too_large');
    expect(sink.issues).toEqual([]);
  });

  it('maps each reason to its code', () => {
    const sink = new IssueSink();
    reportReadFailure(sink, 'f', '/x', 'missing', undefined, 'set.file_too_large');
    reportReadFailure(sink, 'f', '/x', 'too_large', 'file', 'provenance.file_too_large');
    reportReadFailure(sink, 'f', '/x', 'changed', 'file', 'set.file_too_large');
    expect(sink.issues.map((i) => i.code)).toEqual([
      'set.missing_file',
      'provenance.file_too_large',
      'set.unexpected_entry',
    ]);
  });
});
