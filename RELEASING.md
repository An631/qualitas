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
"qualitas": minor
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

### Step 4: Automatic npm publish

When the version PR is merged to `main`, the `release.yml` action runs again.
This time it finds **no pending changesets** but detects that the version in
`package.json` is **newer than what's on npm**, so it runs `changeset publish`
which publishes to npm.

### Step 5: Tag and build binaries (optional)

After the npm publish, create a git tag for the GitHub Release:

```bash
git pull origin main
git tag v0.2.0
git push origin v0.2.0
```

This triggers `publish.yml` which builds native CLI binaries for all 5 platforms
and creates a GitHub Release with downloadable archives.

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
# 6. npm publish happens automatically
# 7. Optionally tag for GitHub Release
git pull && git tag v<new-version> && git push origin v<new-version>
```

## What If I Forget to Add a Changeset?

- The CI `changeset` job on PRs will warn you
- If you push to main without a changeset, nothing happens — no version bump,
  no publish. Your changes are on main but not released.
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

| Change Type | Bump | Example |
|-------------|------|---------|
| Bug fix, typo, internal refactor | `patch` | 0.1.0 → 0.1.1 |
| New feature, new flag, new metric | `minor` | 0.1.0 → 0.2.0 |
| Breaking API change, removed feature | `major` | 0.1.0 → 1.0.0 |

## Workflow Files

| File | Trigger | Purpose |
|------|---------|---------|
| `ci.yml` | Push/PR to main | Lint, test, quality gate |
| `release.yml` | Push to main | Create version PR or publish to npm |
| `publish.yml` | Git tag `v*` | Build cross-platform binaries, GitHub Release |

## Platform Packages

The `@qualitas/binding-*` platform packages (containing native `.node` binaries)
are published separately via the `publish.yml` workflow. The root `qualitas`
package declares them as `optionalDependencies` — npm installs the correct one
for the user's platform automatically.

## Troubleshooting

**"No changesets found" in release action:**
You pushed without a changeset file. Run `npx changeset`, commit, and push.

**"Version X is already published on npm":**
The version in `package.json` matches what's on npm. You need a changeset to
bump the version first.

**"ENEEDAUTH" error:**
The `NPM_TOKEN` secret is missing or expired. Update it in GitHub repo Settings
→ Secrets → Actions.

**Platform package not found for my OS:**
The `@qualitas/binding-*` packages need to be published from CI. Push a `v*` tag
to trigger the build + publish workflow.
