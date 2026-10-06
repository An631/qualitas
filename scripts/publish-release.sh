#!/usr/bin/env bash
# Publishes a release in dependency order: platform packages first, then the root.
# Run by release.yml after the native binaries were copied into npm/*.
#
# The root's optionalDependencies are injected here instead of living in the repo, so
# the lockfile never refers to packages that are not on npm yet, and a published root
# can never point at a binding that is missing.
#
# Safe to re-run: versions that already exist on npm are skipped.
#
# Env: PUBLISH_FLAGS  flags for `npm publish` (default: --provenance --access public)
set -euo pipefail

FLAGS="${PUBLISH_FLAGS:---provenance --access public}"

publish_dir() {
  local dir=$1 log
  log=$(mktemp)
  echo "Publishing $dir..."
  # shellcheck disable=SC2086
  if npm publish "$dir" $FLAGS 2>&1 | tee "$log"; then
    rm -f "$log"
    return 0
  fi
  if grep -q "EPUBLISHCONFLICT\|You cannot publish over\|E409" "$log"; then
    rm -f "$log"
    echo "Skipping $dir (already published)"
    return 0
  fi
  rm -f "$log"
  echo "ERROR: failed to publish $dir" >&2
  return 1
}

# 1. Platform packages. Refuse to publish one that has no binaries in it.
for dir in npm/*/; do
  [ -f "$dir/package.json" ] || continue
  if ! compgen -G "${dir}*.node" > /dev/null || { [ ! -f "${dir}qualitas" ] && [ ! -f "${dir}qualitas.exe" ]; }; then
    echo "ERROR: $dir is missing its .node addon or CLI binary" >&2
    exit 1
  fi
done

for dir in npm/*/; do
  [ -f "$dir/package.json" ] || continue
  publish_dir "./${dir%/}"
done

# 2. Point the root at the exact platform package versions that now exist on npm.
node -e '
  const fs = require("fs");
  const path = require("path");
  const pkg = JSON.parse(fs.readFileSync("package.json", "utf8"));
  const deps = {};
  for (const dir of fs.readdirSync("npm")) {
    const file = path.join("npm", dir, "package.json");
    if (!fs.existsSync(file)) continue;
    const platform = JSON.parse(fs.readFileSync(file, "utf8"));
    if (platform.version !== pkg.version) {
      throw new Error(platform.name + " is " + platform.version + " but the root is " + pkg.version);
    }
    deps[platform.name] = platform.version;
  }
  pkg.optionalDependencies = deps;
  fs.writeFileSync("package.json", JSON.stringify(pkg, null, 2) + "\n");
'

# 3. Root package last.
publish_dir "."
