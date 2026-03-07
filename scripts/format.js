#!/usr/bin/env node
// Format source files.
// Usage:
//   node scripts/format.js                     # format all files
//   node scripts/format.js file1.ts file2.rs   # format only specified files

const { execSync } = require('child_process');
const { extname, join } = require('path');

// Resolve prettier from the project's node_modules so this works
// both via `npm run format` and when called directly (e.g. git hooks).
const prettier = join(__dirname, '..', 'node_modules', '.bin', 'prettier');

function run(cmd) {
  try {
    execSync(cmd, { stdio: 'inherit' });
  } catch {
    // Allow formatter commands to fail gracefully (e.g. cargo not in PATH)
  }
}

const files = process.argv.slice(2);

if (files.length === 0) {
  // No arguments — format everything
  console.log('Formatting rust files...');
  run('cargo fmt');
  console.log('Formatting typescript files...');
  run(`${prettier} --write "js/**/*.ts" "tests/**/*.ts"`);
  console.log('Fixing package.json files...');
  run('npm pkg fix');
  process.exit(0);
}

// Separate files by type
const rsFiles = files.filter((f) => extname(f) === '.rs');
const tsFiles = files.filter((f) => ['.ts', '.js', '.mjs', '.cjs'].includes(extname(f)));
const pkgJsonFiles = files.filter((f) => f.endsWith('package.json'));

// Run `npm pkg fix` in the directory of each staged package.json
for (const pkgFile of pkgJsonFiles) {
  const dir = require('path').dirname(pkgFile) || '.';
  console.log(`Running npm pkg fix in ${dir}...`);
  run(`npm pkg fix --prefix ${dir}`);
}

if (rsFiles.length > 0) {
  console.log('Formatting rust files...');
  run(`rustfmt ${rsFiles.join(' ')}`);
}

if (tsFiles.length > 0) {
  console.log('Formatting typescript files...');
  run(`${prettier} --write ${tsFiles.join(' ')}`);
}
