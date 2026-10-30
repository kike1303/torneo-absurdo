#!/usr/bin/env node
// 008 RF-D9 (plan-d decision 12): the server binary of a release never carries `dev-auth`.
//
//   node scripts/check-release-binary.mjs <binary>               release: no dev-auth strings
//   node scripts/check-release-binary.mjs --expect-dev <binary>  e2e: the dev-auth binary
//
// First the executable header: ELF for a release (it is always the Linux image); ELF or Mach-O
// with --expect-dev, because `just e2e` also runs on macOS. So a wrong file that happens to
// contain `__Host-session` never passes. Then the bytes: a release fails if it contains any of
// the dev-auth strings, and also if it does not contain `__Host-session` (positive control: a
// compressed or empty file never passes). --expect-dev demands the three dev-auth strings.
// Node built-ins only: release.yml runs it with the runner's Node, without setup-js.
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

export const DEV_STRINGS = ['/api/auth/dev/', 'ta_dev_session', 'ta_dev_oauth'];
export const ALWAYS = '__Host-session';

const ELF = Buffer.from([0x7f, 0x45, 0x4c, 0x46]);
// 64- and 32-bit Mach-O in both byte orders, and the universal (fat) header.
const MACH_O = [
  [0xcf, 0xfa, 0xed, 0xfe],
  [0xfe, 0xed, 0xfa, 0xcf],
  [0xce, 0xfa, 0xed, 0xfe],
  [0xfe, 0xed, 0xfa, 0xce],
  [0xca, 0xfe, 0xba, 0xbe],
].map((bytes) => Buffer.from(bytes));

/**
 * @param {Buffer} bytes
 * @param {{ expectDev: boolean }} options
 * @returns {string[]} problems, empty when the binary passes
 */
export function checkBinary(bytes, { expectDev }) {
  const head = bytes.subarray(0, 4);
  const elf = head.equals(ELF);
  const machO = MACH_O.some((magic) => head.equals(magic));
  if (!elf && !(expectDev && machO)) {
    return [expectDev ? 'not an ELF or Mach-O executable' : 'not an ELF executable'];
  }
  const problems = [];
  if (!bytes.includes(ALWAYS)) {
    problems.push(`does not contain ${ALWAYS} (positive control: is this the server binary?)`);
  }
  for (const text of DEV_STRINGS) {
    const found = bytes.includes(text);
    if (expectDev && !found) problems.push(`does not contain ${text}: not built with dev-auth`);
    if (!expectDev && found) problems.push(`contains ${text}: built with dev-auth`);
  }
  return problems;
}

if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  const expectDev = args[0] === '--expect-dev';
  const file = expectDev ? args[1] : args[0];
  if (!file || args.length !== (expectDev ? 2 : 1)) {
    console.error('usage: check-release-binary.mjs [--expect-dev] <binary>');
    process.exit(2);
  }
  let bytes;
  try {
    bytes = readFileSync(file);
  } catch {
    console.error(`check-release-binary: cannot read ${file}`);
    process.exit(1);
  }
  const problems = checkBinary(bytes, { expectDev });
  if (problems.length > 0) {
    console.error(`check-release-binary: ${file}`);
    for (const problem of problems) console.error(`  ${problem}`);
    process.exit(1);
  }
  console.log(`check-release-binary: ${file} ok (${expectDev ? 'dev-auth' : 'release'})`);
}
