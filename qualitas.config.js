// qualitas.config.js — Project-level configuration for qualitas code quality analysis.
//
// All fields are optional. CLI flags override these values.
// See: https://github.com/qualitas-ts/qualitas-ts

module.exports = {
  // Exit code 1 if any function scores below this threshold (0-100)
  threshold: 80,

  // Weight profile: 'default' | 'cc-focused' | 'data-focused' | 'strict'
  profile: 'default',

  // Directories to exclude from analysis. Replaces built-in defaults when set.
  // Built-in defaults (used when omitted): node_modules, dist, build, .git, coverage, target
  // exclude: ['node_modules', 'dist', 'build', '.git', 'coverage', 'target', 'vendor/'],

  // Per-language configuration. Keys are lowercase language names.
  // When a language's testPatterns is set, it replaces that language's built-in defaults entirely.
  // Languages not listed here keep their adapter defaults.
  // Matching: substring match against file name AND full path (not glob, not regex).
  languages: {
    typescript: {
      testPatterns: [
        '.test.', // foo.test.ts, bar.test.tsx
        '.spec.', // foo.spec.ts
        '.playwright-test.', // Playwright test files
        'tests/', // Files under tests/ directories (Unix)
        'tests\\', // Files under tests\ directories (Windows)
        'fixtures/', // Test fixture data (Unix)
        'fixtures\\', // Test fixture data (Windows)
      ],
    },
    rust: {
      testPatterns: [
        '_test.rs', // foo_test.rs
        '_tests.rs', // foo_tests.rs
        'tests/', // Files under tests/ directories (Unix)
        'tests\\', // Files under tests\ directories (Windows)
      ],
    },
  },
};
