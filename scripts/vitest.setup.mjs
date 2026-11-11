// TA-80: under the pre-push hook git exports GIT_DIR and other GIT_* variables, which would point
// every `git` a test spawns at the real repository. They are removed before any test runs; the
// tests that spawn git also pass `cleanGitEnv()` themselves (scripts/git-env.mjs).
for (const name of Object.keys(process.env)) {
  if (name.startsWith('GIT_')) delete process.env[name];
}
