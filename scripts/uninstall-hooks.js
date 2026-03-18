#!/usr/bin/env node
// Removes project hooks from .git/hooks/, restoring default (no-op) state.
// Run with: npm run git-hooks:uninstall

const { unlinkSync, existsSync, readdirSync } = require('fs');
const { join } = require('path');

const hooksDir = join(__dirname, '..', 'hooks');
const gitHooksDir = join(__dirname, '..', '.git', 'hooks');

if (!existsSync(gitHooksDir)) {
  process.exit(0);
}

let removed = 0;
for (const file of readdirSync(hooksDir)) {
  const dest = join(gitHooksDir, file);
  if (existsSync(dest)) {
    unlinkSync(dest);
    removed++;
  }
}

console.log(`Git hooks uninstalled (${removed} hook(s) removed).`);
