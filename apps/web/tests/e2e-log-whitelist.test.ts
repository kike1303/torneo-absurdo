import { describe, expect, it } from 'vitest';
import { CANARIES, whitelistProblems } from '../e2e/support/logs.js';

const START = '{"ts":"t","level":"info","event":"web_started"}';
const LISTEN = 'Listening on http://0.0.0.0:4173';
const ERROR =
  '{"ts":"t","level":"error","event":"unhandled_error","error_id":"x","route_id":"/dev/error-probe","status":500}';

describe('e2e log whitelist (plan-a decision 12)', () => {
  it('accepts startup, one Listening line and allowed JSON lines', () => {
    expect(whitelistProblems([START, LISTEN, ERROR])).toEqual([]);
  });

  it('rejects any other output', () => {
    expect(whitelistProblems([START, LISTEN, '(node:1) Warning: something'])).not.toEqual([]);
    expect(
      whitelistProblems([START, LISTEN, '{"ts":"t","level":"info","event":"x","url":"/es/"}']),
    ).not.toEqual([]);
    expect(whitelistProblems([START, LISTEN, LISTEN])).not.toEqual([]);
    expect(whitelistProblems([START, ERROR, LISTEN])).not.toEqual([]);
    expect(whitelistProblems([START])).not.toEqual([]);
  });

  it('rejects canaries anywhere', () => {
    for (const canary of Object.values(CANARIES)) {
      const line = `{"ts":"t","level":"error","event":"e","error_message":"${canary}"}`;
      expect(whitelistProblems([START, LISTEN, line])).not.toEqual([]);
    }
  });
});
