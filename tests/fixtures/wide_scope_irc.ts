// Expected: high IRC (wide-scope variables referenced many times),
// HIGH_IDENTIFIER_CHURN flag, IRC_ERROR threshold exceeded.

export function generateReport(records: any[]): string {
  const sections: string[] = [];
  const summary: Record<string, number> = {};
  const errors: string[] = [];

  for (const record of records) {
    if (record.type === 'sale') {
      const key = record.category ?? 'unknown';
      summary[key] = (summary[key] ?? 0) + record.amount;
      sections.push(`Sale: ${record.id} — ${record.amount}`);
    } else if (record.type === 'refund') {
      const key = record.category ?? 'unknown';
      summary[key] = (summary[key] ?? 0) - record.amount;
      sections.push(`Refund: ${record.id} — ${record.amount}`);
    } else if (record.type === 'adjustment') {
      sections.push(`Adj: ${record.id}`);
    } else {
      errors.push(`Unknown type: ${record.type} for ${record.id}`);
    }
  }

  const header = `Report (${sections.length} items, ${errors.length} errors)`;
  const summaryLines = Object.entries(summary)
    .map(([k, v]) => `  ${k}: ${v}`)
    .join('\n');
  const errorSection =
    errors.length > 0 ? '\nErrors:\n' + errors.map((e) => '  ' + e).join('\n') : '';

  return [header, summaryLines, ...sections, errorSection].filter(Boolean).join('\n');
}
