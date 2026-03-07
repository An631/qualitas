# Changesets

This folder is managed by [Changesets](https://github.com/changesets/changesets).

Before merging a PR that changes user-facing behavior, run:

```bash
npx changeset
```

This creates a change description file. At release time, run:

```bash
npx changeset version   # consumes changesets, bumps version, updates CHANGELOG.md
npx changeset publish   # publishes to npm
```
