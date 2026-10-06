# Changesets

This folder is managed by [Changesets](https://github.com/changesets/changesets).

Before merging a PR that changes user-facing behavior, run:

```bash
npx changeset
```

This creates a change description file. The `release.yml` workflow consumes changesets and creates the version PR. Once
that PR is merged, it builds and publishes the platform binding packages first,
adds their exact versions to the root package manifest, and publishes the root
package last. You do not need to run the publish command manually.
