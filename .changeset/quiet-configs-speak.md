---
'qualitas': patch
---

Fail with exit code 2 and a hint when a config file exports nothing (for example `module.exports = {...}` in a `"type": "module"` package) instead of silently using defaults. Fixes #46.
