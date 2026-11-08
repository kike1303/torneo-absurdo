import { CryptoUnavailableError, type VerifyDeps, type VerifyEngine } from './verify.js';

// The real `deps` of `verifyFight`, and the only importer of `engine-wasm/`, only with `import()`
// (011 RF5): the `.wasm` is a file of the same origin that the `fetch` of the glue downloads, and
// only a `decided` fight past its commit and RF2 ever asks for it.

const LOAD_TIMEOUT_MS = 30_000;

let loading: Promise<VerifyEngine> | null = null;

async function load(): Promise<VerifyEngine> {
  const [glue, wasm] = await Promise.all([
    import('./engine-wasm/engine_wasm.js'),
    import('./engine-wasm/engine_wasm_bg.wasm?url'),
  ]);
  await glue.default({ module_or_path: wasm.default });
  return { simulate: glue.simulate, rulesetBounds: glue.ruleset_bounds };
}

function withDeadline(work: Promise<VerifyEngine>): Promise<VerifyEngine> {
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => reject(new Error('engine_timeout')), LOAD_TIMEOUT_MS);
    work.then(resolve, reject).finally(() => clearTimeout(timer));
  });
}

function engine(): Promise<VerifyEngine> {
  // A failure is not kept: "retry" has to try again (RF15).
  loading ??= withDeadline(load()).catch((error: unknown) => {
    loading = null;
    throw error;
  });
  return loading;
}

async function sha256(bytes: Uint8Array<ArrayBuffer>): Promise<ArrayBuffer> {
  // `undefined` outside a secure context, whatever the types say.
  const subtle = (globalThis.crypto as Crypto | undefined)?.subtle as SubtleCrypto | undefined;
  if (subtle === undefined) throw new CryptoUnavailableError();
  return subtle.digest('SHA-256', bytes);
}

export function realDeps(): VerifyDeps {
  return { sha256, engine };
}
