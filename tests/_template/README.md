# Adding Tests for a New Language

When you add a new language adapter to qualitas, create a matching test suite here.

## Steps

1. Copy this `_template/` directory to `tests/<your-language>/`
2. Rename `adapter.test.template.ts` to `adapter.test.ts`
3. Create a `fixtures/` subdirectory with source files in your language
4. Fill in the test file with language-specific source strings
5. Run `npm test` to verify

## Required Fixtures

Create these fixture categories (adapt the patterns to your language):

| Fixture | Purpose | Expected behavior |
| --------- | --------- | ------------------- |
| `clean.<ext>` | Simple, well-structured functions | Score >= 80, grade A, no refactoring flags |
| `deeply_nested.<ext>` | 5+ levels of control flow nesting | Score < 65, HIGH_COGNITIVE_FLOW flag |
| `data_heavy.<ext>` | Many operators/operands (math-heavy) | HIGH_DATA_COMPLEXITY flag |
| `wide_scope_irc.<ext>` | Variables referenced across wide scope | HIGH_IDENTIFIER_CHURN flag |

## Test Categories

Your adapter test should cover:

1. **Clean code** — simple functions score well
2. **Complex code** — nested/long/many-param functions get flagged
3. **Line numbers** — `SourceLocation` reports 1-based line numbers
4. **Function collection** — all function patterns in your language are found
5. **Dependency coupling** — import/require patterns are detected (if applicable)

## Running Tests

```bash
# Run all tests
npm test

# Run only your language's tests
npx jest tests/<your-language>

# Run shared + your language
npx jest tests/shared tests/<your-language>
```
