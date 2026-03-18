#!/usr/bin/env node
// Copies tracked hooks from hooks/ into .git/hooks/ so they're active locally.
// Runs automatically via the "prepare" lifecycle script on `npm install`.

const { copyFileSync, chmodSync, existsSync, readdirSync } = require('fs');
const { join } = require('path');

const hooksDir = join(__dirname, '..', 'hooks');
const gitHooksDir = join(__dirname, '..', '.git', 'hooks');

if (!existsSync(gitHooksDir)) {
  // Not inside a git repo (e.g. installed as a dependency) — skip silently.
  process.exit(0);
}

for (const file of readdirSync(hooksDir)) {
  const src = join(hooksDir, file);
  const dest = join(gitHooksDir, file);
  copyFileSync(src, dest);
  try {
    chmodSync(dest, 0o755);
  } catch {
    // chmod may fail on Windows — hooks still work without it.
  }
}

console.log('Git hooks installed from hooks/ directory.');
