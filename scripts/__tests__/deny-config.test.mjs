import { readFileSync } from 'node:fs';
import { describe, expect, it } from 'vitest';

// plan-c deviation 22: cargo-deny skips dev-dependencies when it counts versions unless
// `multiple-versions-include-dev` is on, so a second tungstenite brought by a dev-dependency
// (tokio-tungstenite in the server tests) passed `just deny`. Running cargo-deny itself needs the
// registry, so this test pins the configuration and the reproduction lives in the deviation.
const deny = readFileSync(new URL('../../deny.toml', import.meta.url), 'utf8');

/** Body of a top-level TOML table, up to the next `[table]` header. */
function table(name) {
  const start = deny.indexOf(`\n[${name}]\n`);
  if (start === -1) throw new Error(`deny.toml has no [${name}]`);
  const rest = deny.slice(start + name.length + 4);
  const end = rest.search(/^\[/m);
  return end === -1 ? rest : rest.slice(0, end);
}

describe('deny.toml (RF-B2, plan-c deviation 22)', () => {
  const bans = table('bans');

  it('counts dev-dependencies when it looks for duplicate versions', () => {
    expect(bans).toMatch(/^multiple-versions-include-dev = true$/m);
  });

  it('denies a second tungstenite', () => {
    expect(bans).toMatch(/\{ crate = "tungstenite", deny-multiple-versions = true \}/);
  });

  it('does not skip tungstenite anywhere', () => {
    expect(deny).not.toMatch(/skip(-tree)?\s*=\s*\[[^\]]*tungstenite/);
  });

  it('does not drop dev-dependencies from the graph', () => {
    expect(table('graph')).not.toMatch(/^exclude-dev\s*=\s*true/m);
  });
});
