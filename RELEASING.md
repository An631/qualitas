# Releasing Qualitas

This document explains how to release new versions of the qualitas npm package.

## How Versioning Works

Qualitas uses [Changesets](https://github.com/changesets/changesets) to manage
versions and changelogs. The process has three phases:

1. **Add changesets** — describe what changed (during development)
2. **Version** — consume changesets to bump version + update CHANGELOG (automated via PR)
3. **Publish** — push the new version to npm (automated on merge)

## Step-by-Step Release Process

### Step 1: Add a changeset when you make changes

After making changes that affect users (new features, bug fixes, breaking
changes), run:

```bash
npx changeset
```

This interactive prompt asks:

1. **Which package?** — select `qualitas`
2. **What kind of change?** — `patch` (bug fix), `minor` (new feature), or `major` (breaking)
3. **Summary** — a short description of what changed

This creates a file in `.changeset/` (e.g., `.changeset/happy-dogs-fly.md`)
with content like:

```markdown
---
'qualitas': minor
---

Added match arm CFC discount to reduce false positives on exhaustive match statements
```

**Commit this file with your code changes.** You can have multiple changeset
files — they accumulate until release time.

### Step 2: Push to main

Push your changes (with the changeset file) to the `main` branch. This triggers
the `release.yml` GitHub Action which:

1. Detects pending changeset files in `.changeset/`
2. Creates a **"chore: version packages"** pull request that:
   - Deletes the changeset files
   - Bumps the version in `package.json` (e.g., 0.1.0 → 0.2.0)
   - Updates `CHANGELOG.md` with the changeset descriptions

### Step 3: Review and merge the version PR

Review the auto-generated PR. Check that:

- The version bump is correct (patch/minor/major)
- The CHANGELOG entry looks good

Merge it.

### Step 4: Automatic build and publish

When the version PR is merged to `main`, the `release.yml` action runs again.
With no pending changesets, it checks whether this package version is already
on npm. If not, it builds the native addon and standalone CLI for all supported
platforms, then publishes the platform packages first and the root package last.
The workflow injects the exact platform package versions into the root
`optionalDependencies` immediately before publishing it. It then creates the
`v<version>` tag and GitHub Release.

## Quick Reference

```bash
# 1. Make your code changes
git add .
git commit -m "feat: add new feature"

# 2. Add a changeset describing the change
npx changeset
git add .changeset/
git commit -m "chore: add changeset"

# 3. Push to main
git push origin main

# 4. Wait for the "chore: version packages" PR to appear
# 5. Review and merge it
# 6. The release workflow builds and publishes bindings, then the root package
# 7. The workflow creates the version tag and GitHub Release
```

## What If I Forget to Add a Changeset?

- The CI `changeset` job on PRs will warn you
- If you push to main without a changeset, nothing happens — no version bump,
  no version PR or publish. Your changes are on main but not released.
- Just run `npx changeset` later, commit, and push. The version PR will appear.

## What If I Need to Publish Without Changes?

If you need to re-publish or force a version bump:

```bash
npx changeset add --empty
git add .changeset/
git commit -m "chore: empty changeset for release"
git push origin main
```

## Versioning Guide

| Change Type                          | Bump    | Example       |
| ------------------------------------ | ------- | ------------- |
| Bug fix, typo, internal refactor     | `patch` | 0.1.0 → 0.1.1 |
| New feature, new flag, new metric    | `minor` | 0.1.0 → 0.2.0 |
| Breaking API change, removed feature | `major` | 0.1.0 → 1.0.0 |

## Workflow Files

| File          | Trigger         | Purpose                                                                                                      |
| ------------- | --------------- | ------------------------------------------------------------------------------------------------------------ |
| `ci.yml`      | Push/PR to main | Lint, test, quality gate                                                                                     |
| `release.yml` | Push to main    | Create version PR; otherwise build binaries, publish platform packages, then root package and GitHub Release |

## Platform Packages

The `@qualitas/binding-*` platform package manifests live under `npm/*`. They
are not dependencies in the checked-in root `package.json` or lockfile, so
`npm ci` can validate a stable lockfile before and after a release. During a
release, `scripts/publish-release.sh` publishes the five platform packages
first, injects their exact versions as the root package's `optionalDependencies`,
and publishes the root package last. This ordering ensures every binding the
published root refers to is already available on npm.

## Troubleshooting

**"No changesets found" in release action:**
You pushed without a changeset file. Run `npx changeset`, commit, and push.

**"Version X is already published on npm":**
The release workflow skips versions already published with a GitHub Release.
If a release was interrupted after publishing, rerun the workflow; it resumes
the remaining steps and skips package versions already on npm.

**"ENEEDAUTH" error:**
The `NPM_TOKEN` secret is missing or expired. Update it in GitHub repo Settings
→ Secrets → Actions.

**Platform package not found for my OS:**
The release publishes the root package only after its platform binding packages
are available. If installation still fails, check the `build-native` and
`publish` jobs in the `release.yml` run for that version.
