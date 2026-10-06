#!/usr/bin/env bash
# Called by changesets/action during the version step.
# 1. Bump root package version via changesets
# 2. Sync Cargo CLI and platform package versions
# 3. Update package-lock.json

npx changeset version

# Read the new version from root package.json
VERSION=$(node -p "require('./package.json').version")
echo "Syncing platform packages to version $VERSION"

# Keep the standalone CLI's clap version and lockfile in sync with the npm package.
VERSION="$VERSION" node -e '
  const fs = require("fs");
  const files = [
    ["crates/qualitas-cli/Cargo.toml", /(^version = ")[^"]+(")/m],
    ["Cargo.lock", /(\[\[package\]\]\r?\nname = "qualitas-cli"\r?\nversion = ")[^"]+(")/],
  ];
  for (const [path, pattern] of files) {
    const contents = fs.readFileSync(path, "utf8");
    if (!pattern.test(contents)) {
      throw new Error("Could not find qualitas-cli version in " + path);
    }
    fs.writeFileSync(
      path,
      contents.replace(pattern, "$1" + process.env.VERSION + "$2")
    );
  }
'

# Update all platform package versions to match
for pkg in npm/*/package.json; do
  node -e "
    const fs = require('fs');
    const pkg = JSON.parse(fs.readFileSync('$pkg', 'utf8'));
    pkg.version = '$VERSION';
    fs.writeFileSync('$pkg', JSON.stringify(pkg, null, 2) + '\n');
  "
done

# Sync optionalDependencies ranges in root package.json to ^<new-version>
node -e "
  const fs = require('fs');
  const pkg = JSON.parse(fs.readFileSync('package.json', 'utf8'));
  if (pkg.optionalDependencies) {
    for (const name of Object.keys(pkg.optionalDependencies)) {
      pkg.optionalDependencies[name] = '^$VERSION';
    }
    fs.writeFileSync('package.json', JSON.stringify(pkg, null, 2) + '\n');
  }
"

npm install --omit=optional --package-lock-only

# The new platform bindings are published after this PR merges, so npm cannot resolve
# them yet and may drop their lockfile entries. Keep version-less placeholders so
# `npm ci` still works until the bindings are published.
node -e '
  const fs = require("fs");
  const lock = JSON.parse(fs.readFileSync("package-lock.json", "utf8"));
  const names = Object.keys(lock.packages[""].optionalDependencies || {}).map(
    (n) => "node_modules/" + n
  );
  const out = {};
  for (const [key, val] of Object.entries(lock.packages)) {
    if (key === "node_modules/@sinclair/typebox") {
      for (const n of names) out[n] = { optional: true };
    }
    if (!names.includes(key)) out[key] = val;
  }
  for (const n of names) out[n] = out[n] || { optional: true };
  lock.packages = out;
  fs.writeFileSync("package-lock.json", JSON.stringify(lock, null, 2) + "\n");
'
