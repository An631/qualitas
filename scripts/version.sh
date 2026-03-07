#!/usr/bin/env bash
# Called by changesets/action during the version step.
# 1. Bump root package version via changesets
# 2. Sync platform package versions to match
# 3. Update package-lock.json

npx changeset version

# Read the new version from root package.json
VERSION=$(node -p "require('./package.json').version")
echo "Syncing platform packages to version $VERSION"

# Update all platform package versions to match
for pkg in npm/*/package.json; do
  node -e "
    const fs = require('fs');
    const pkg = JSON.parse(fs.readFileSync('$pkg', 'utf8'));
    pkg.version = '$VERSION';
    fs.writeFileSync('$pkg', JSON.stringify(pkg, null, 2) + '\n');
  "
done

npm install --omit=optional --package-lock-only
