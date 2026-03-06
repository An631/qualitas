// qualitas.config.js — Project-level configuration for qualitas code quality analysis.
//
// All fields are optional. CLI flags override these values.
// See: https://github.com/qualitas/qualitas

module.exports = {
  // Exit code 1 if any function scores below this threshold (0-100)
  threshold: 80,

  // Directories and files to exclude from analysis.
  // Replaces built-in defaults (node_modules, dist, build, .git, coverage, target).
  exclude: [
    'node_modules',
    'dist',
    'build',
    '.git',
    'coverage',
    'target',
    'qualitas_napi.js', // Auto-generated NAPI binding loader
  ],

  // Weight profile: 'default' | 'cc-focused' | 'data-focused' | 'strict'
  profile: 'default',

  // Fail (exit 1) if any function has flags at this severity or above.
  // 'warn'  → fail on any warning or error flag (zero tolerance)
  // 'error' → fail only on error-level flags
  // Omit or set to false to disable (default: score-only threshold)
  // failOnFlags: 'error',

  /** Flag configuration. Each flag can be:
   *   true            → enabled with default thresholds
   *   false           → disabled
   *   { warn, error } → enabled with custom thresholds
   * Flags not listed use their built-in defaults (all enabled except excessiveReturns).
   */
  // flags: {
  //   tooManyParams: { warn: 5, error: 7 },
  //   tooLong: { warn: 60, error: 100 },
  //   excessiveReturns: true, // re-enable (disabled by default)
  // },

  /**
   * Directories to exclude from analysis. Replaces built-in defaults when set.
   * Built-in defaults (used when omitted): node_modules, dist, build, .git, coverage, target
   * exclude: ['node_modules', 'dist', 'build', '.git', 'coverage', 'target', 'vendor/'],
   *
   * Per-language configuration. Keys are lowercase language names.
   * When a language's testPatterns is set, it replaces that language's built-in defaults entirely.
   * Languages not listed here keep their adapter defaults.
   * Matching: substring match against file name AND full path (not glob, not regex).
   */
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
