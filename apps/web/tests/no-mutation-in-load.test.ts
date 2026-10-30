import { readdirSync, readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';
import { describe, expect, it } from 'vitest';

// 008 RF-D3 and ADR-0008 D5 (plan-d decision 8): no `load` and no `GET` calls an API mutation.
// ESLint keeps the import of `api/mutations` inside `+page.server.ts`; this test checks the use:
// every call to a function imported from there sits inside `export const actions`. It also checks
// that `reads.ts` only calls the client with `GET` and that no `+server.ts` exports an unsafe
// method.
const WEB = fileURLToPath(new URL('..', import.meta.url));
const SRC = path.join(WEB, 'src');

const MUTATIONS = /(?:^#lib\/server\/api\/mutations|\/server\/api\/mutations)(?:\.js|\.ts)?$/;
const UNSAFE = ['POST', 'PUT', 'PATCH', 'DELETE'];

function parse(file: string, text: string): ts.SourceFile {
  return ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true, ts.ScriptKind.TS);
}

function visit(node: ts.Node, fn: (node: ts.Node) => void): void {
  fn(node);
  node.forEachChild((child) => visit(child, fn));
}

/** The local names bound by imports from `api/mutations`. */
function mutationImports(source: ts.SourceFile): Set<string> {
  const names = new Set<string>();
  for (const statement of source.statements) {
    if (!ts.isImportDeclaration(statement)) continue;
    if (!ts.isStringLiteral(statement.moduleSpecifier)) continue;
    if (!MUTATIONS.test(statement.moduleSpecifier.text)) continue;
    const clause = statement.importClause;
    if (!clause || clause.isTypeOnly) continue;
    if (clause.name) names.add(clause.name.text);
    const bindings = clause.namedBindings;
    if (bindings && ts.isNamespaceImport(bindings)) names.add(bindings.name.text);
    if (bindings && ts.isNamedImports(bindings)) {
      for (const element of bindings.elements) {
        if (!element.isTypeOnly) names.add(element.name.text);
      }
    }
  }
  return names;
}

/** The initializer of `export const actions = …`, if any. */
function actionsObject(source: ts.SourceFile): ts.Node | undefined {
  for (const statement of source.statements) {
    if (!ts.isVariableStatement(statement)) continue;
    const exported = statement.modifiers?.some((m) => m.kind === ts.SyntaxKind.ExportKeyword);
    if (!exported) continue;
    for (const declaration of statement.declarationList.declarations) {
      if (ts.isIdentifier(declaration.name) && declaration.name.text === 'actions') {
        return declaration.initializer;
      }
    }
  }
  return undefined;
}

function inside(node: ts.Node, container: ts.Node | undefined): boolean {
  if (!container) return false;
  return node.getStart() >= container.getStart() && node.getEnd() <= container.getEnd();
}

/** Problems of one `+page.server.ts`: a use of a mutation outside `actions`. */
export function pageServerProblems(file: string, text: string): string[] {
  const source = parse(file, text);
  const names = mutationImports(source);
  if (names.size === 0) return [];
  const actions = actionsObject(source);
  const problems: string[] = [];
  visit(source, (node) => {
    if (!ts.isIdentifier(node) || !names.has(node.text)) return;
    // The binding in the import itself.
    if (ts.isImportSpecifier(node.parent) || ts.isImportClause(node.parent)) return;
    if (ts.isNamespaceImport(node.parent)) return;
    if (!inside(node, actions)) {
      const { line } = source.getLineAndCharacterOfPosition(node.getStart());
      problems.push(`${file}:${line + 1}: ${node.text} used outside \`export const actions\``);
    }
  });
  return problems;
}

/** Problems of `reads.ts`: a call to the client with a method other than `'GET'`. */
export function readsProblems(file: string, text: string): string[] {
  const source = parse(file, text);
  const problems: string[] = [];
  visit(source, (node) => {
    if (!ts.isCallExpression(node)) return;
    if (!ts.isIdentifier(node.expression) || node.expression.text !== 'apiRequest') return;
    const method = node.arguments[1];
    if (!method || !ts.isStringLiteral(method) || method.text !== 'GET') {
      const { line } = source.getLineAndCharacterOfPosition(node.getStart());
      problems.push(`${file}:${line + 1}: apiRequest without a literal 'GET'`);
    }
  });
  return problems;
}

/** Problems of a `+server.ts`: an exported unsafe method. */
export function endpointProblems(file: string, text: string): string[] {
  const source = parse(file, text);
  const problems: string[] = [];
  const flag = (name: string) => {
    if (UNSAFE.includes(name) || name === 'fallback') problems.push(`${file}: exports ${name}`);
  };
  for (const statement of source.statements) {
    const exported = ts.canHaveModifiers(statement)
      ? ts.getModifiers(statement)?.some((m) => m.kind === ts.SyntaxKind.ExportKeyword)
      : false;
    if (exported && ts.isFunctionDeclaration(statement) && statement.name) {
      flag(statement.name.text);
    }
    if (exported && ts.isVariableStatement(statement)) {
      for (const d of statement.declarationList.declarations) {
        if (ts.isIdentifier(d.name)) flag(d.name.text);
      }
    }
    if (ts.isExportDeclaration(statement) && statement.exportClause) {
      if (ts.isNamedExports(statement.exportClause)) {
        for (const element of statement.exportClause.elements) flag(element.name.text);
      }
    }
  }
  return problems;
}

/** Problems of any other file of `src/`: it imports `api/mutations` at all. */
export function importProblems(file: string, text: string): string[] {
  return mutationImports(parse(file, text)).size > 0 ? [`${file}: imports api/mutations`] : [];
}

function walk(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name !== 'paraglide') walk(full, out);
    } else if (/\.(?:ts|js)$/.test(entry.name)) out.push(full);
  }
  return out;
}

/** Every problem of the source tree. */
function treeProblems(): string[] {
  const problems: string[] = [];
  for (const full of walk(SRC)) {
    const file = path.relative(WEB, full).split(path.sep).join('/');
    const text = readFileSync(full, 'utf8');
    if (file.endsWith('.test.ts')) continue;
    if (file === 'src/lib/server/api/reads.ts') problems.push(...readsProblems(file, text));
    if (/^src\/routes\/(?:.*\/)?\+page\.server\.ts$/.test(file)) {
      problems.push(...pageServerProblems(file, text));
    } else if (!file.startsWith('src/lib/server/api/')) {
      problems.push(...importProblems(file, text));
    }
    if (/^src\/routes\/(?:.*\/)?\+server\.ts$/.test(file)) {
      problems.push(...endpointProblems(file, text));
    }
  }
  return problems;
}

describe('no API mutation from a load (008 RF-D3, D5)', () => {
  it('the source tree has no problem', () => {
    expect(treeProblems()).toEqual([]);
  });

  it('the tree does have a page that uses mutations, so the check is not empty', () => {
    const pages = walk(path.join(SRC, 'routes')).filter((f) => f.endsWith('+page.server.ts'));
    const users = pages.filter((f) => mutationImports(parse(f, readFileSync(f, 'utf8'))).size > 0);
    expect(users.length).toBeGreaterThan(0);
  });
});

// The three mutants of plan-d decision 8, kept as fixtures.
const PAGE = `import { putDisplayName } from '#lib/server/api/mutations.js';
import { internalApi } from '#lib/server/api/context.js';
export const load = async ({ fetch }) => {
  BODY_LOAD
};
export const actions = {
  rename: async ({ fetch }) => putDisplayName(internalApi(fetch), 'x'),
};
`;

describe('the checks catch each form', () => {
  it('a page that only calls the mutation inside actions passes', () => {
    expect(pageServerProblems('p.ts', PAGE.replace('BODY_LOAD', 'return {};'))).toEqual([]);
  });

  it.each([
    ['a call in the load', "await putDisplayName(internalApi(fetch), 'x');"],
    ['the function passed as a value', 'const f = putDisplayName; return { f };'],
  ])('mutant: %s', (_name, body) => {
    expect(pageServerProblems('p.ts', PAGE.replace('BODY_LOAD', body))).toHaveLength(1);
  });

  it('mutant: a call in a helper outside actions', () => {
    const text = PAGE.replace('BODY_LOAD', 'return {};').concat(
      "\nconst helper = (fetch) => putDisplayName(internalApi(fetch), 'x');\n",
    );
    expect(pageServerProblems('p.ts', text)).toHaveLength(1);
  });

  it('a namespace import is followed too', () => {
    const text = `import * as mut from '../lib/server/api/mutations.js';
export const load = ({ fetch }) => mut.putPreferences({ fetch, base: '' }, {});
export const actions = {};
`;
    expect(pageServerProblems('p.ts', text)).toHaveLength(1);
  });

  it('mutant: an import in +layout.server.ts', () => {
    const text =
      "import { postUnlink } from '#lib/server/api/mutations.js';\nexport const load = () => ({});\n";
    expect(importProblems('src/routes/+layout.server.ts', text)).toHaveLength(1);
    expect(
      importProblems(
        'src/routes/+layout.server.ts',
        "import type { X } from '#lib/server/api/mutations.js';\n",
      ),
    ).toEqual([]);
  });

  it("mutant: a 'PUT' in reads.ts", () => {
    const text = `import { apiRequest } from './client.js';
export const getMe = (api) => apiRequest(api, 'GET', '/api/me');
export const sneaky = (api) => apiRequest(api, 'PUT', '/api/me/preferences', { body: {} });
export const dynamic = (api, m) => apiRequest(api, m, '/api/me');
`;
    expect(readsProblems('reads.ts', text)).toHaveLength(2);
  });

  it.each([
    'export const POST = () => new Response();',
    'export function PUT() { return new Response(); }',
    'const DELETE = () => new Response();\nexport { DELETE };',
    'export const fallback = () => new Response();',
  ])('an unsafe +server.ts: %s', (text) => {
    expect(endpointProblems('src/routes/x/+server.ts', text)).toHaveLength(1);
  });

  it('a GET +server.ts passes', () => {
    expect(
      endpointProblems('src/routes/healthz/+server.ts', 'export const GET = () => new Response();'),
    ).toEqual([]);
  });
});
