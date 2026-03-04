// qualitas.config.js — Project-level configuration for qualitas code quality analysis.
//
// All fields are optional. CLI flags override these values.
// See: https://github.com/qualitas-ts/qualitas-ts

module.exports = {
  // Exit code 1 if any function scores below this threshold (0-100)
  threshold: 80,

  // Weight profile: 'default' | 'cc-focused' | 'data-focused' | 'strict'
  profile: 'default',

  // Directories/patterns to exclude from analysis
  exclude: ['vendor/', 'generated/'],
};
