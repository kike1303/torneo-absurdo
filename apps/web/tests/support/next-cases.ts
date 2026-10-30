// The shared table of `next` (008 plan-d decision 9, change 7 of architect): every consumer
// (`/login`, `/welcome`, `/onboarding` and `to` of the language action) runs it. `null` means
// "falls back to /<locale>/".
export const NEXT_CASES: [string | null | undefined, string | null][] = [
  ['/es/account', '/es/account'],
  ['/en/account?tab=1', '/en/account?tab=1'],
  ['/es/', '/es/'],
  ['/en/dev/components?theme=light#x', '/en/dev/components?theme=light#x'],
  ['/es/%2F%2Fevil', '/es/%2F%2Fevil'],
  // No locale prefix.
  ['/es', null],
  ['/account', null],
  ['/', null],
  ['/fr/x', null],
  ['/%2F%2Fevil', null],
  // Another origin, protocol-relative or not a path.
  ['//evil.example', null],
  ['//evil.example/es/', null],
  ['/\\evil.example', null],
  ['/\\/evil.example', null],
  ['https://evil.example', null],
  ['https://evil.example/es/', null],
  ['javascript:alert(1)', null],
  ['es/account', null],
  [' /es/account', null],
  // Control characters (C0, DEL, C1): a `Location` cannot carry them.
  ...Array.from(
    { length: 0x20 },
    (_, c) => [`/es/a${String.fromCharCode(c)}b`, null] as [string, null],
  ),
  ['/es/a\u007fb', null],
  ['/es/a\u0085b', null],
  ['/es/a\u009fb', null],
  ['', null],
  [null, null],
  [undefined, null],
  [`/es/${'a'.repeat(600)}`, null],
];
