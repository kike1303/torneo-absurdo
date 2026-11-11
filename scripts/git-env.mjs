// TA-80: git exports GIT_DIR, GIT_WORK_TREE, GIT_INDEX_FILE and more to its hooks, so a `git`
// spawned under the pre-push hook (`just check`) acts on the real repository whatever its `cwd`.
// A child that must work on the repository in its `cwd` gets an environment without any GIT_*.

/**
 * A copy of `env` without any GIT_* variable.
 * @param {NodeJS.ProcessEnv} [env]
 * @returns {NodeJS.ProcessEnv}
 */
export function cleanGitEnv(env = process.env) {
  return Object.fromEntries(Object.entries(env).filter(([name]) => !name.startsWith('GIT_')));
}
