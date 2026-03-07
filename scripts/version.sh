#!/usr/bin/env bash
# Called by changesets/action during the version step.
# Bumps version and syncs package-lock.json.
npx changeset version
npm install --omit=optional --package-lock-only
