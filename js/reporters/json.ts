import type { FileQualityReport, ProjectQualityReport } from '../types.js';

export function renderJsonReport(report: FileQualityReport | ProjectQualityReport): string {
  return JSON.stringify(report, null, 2);
}
