// Expected: high DCI (many operators/operands), moderate CFC
// Should trigger HIGH_DATA_COMPLEXITY or HIGH_HALSTEAD_EFFORT

export function computeMetrics(data: number[]) {
  const n = data.length;
  const sum = data.reduce((acc, val) => acc + val, 0);
  const mean = sum / n;
  const variance = data.reduce((acc, val) => acc + (val - mean) ** 2, 0) / n;
  const stddev = Math.sqrt(variance);
  const min = Math.min(...data);
  const max = Math.max(...data);
  const range = max - min;
  const sortedData = [...data].sort((a, b) => a - b);
  const medianIndex = Math.floor(n / 2);
  const median = n % 2 === 0
    ? (sortedData[medianIndex - 1]! + sortedData[medianIndex]!) / 2
    : sortedData[medianIndex]!;
  const q1 = sortedData[Math.floor(n / 4)]!;
  const q3 = sortedData[Math.floor((3 * n) / 4)]!;
  const iqr = q3 - q1;
  const skewness = data.reduce((acc, val) => acc + ((val - mean) / stddev) ** 3, 0) / n;
  const kurtosis = data.reduce((acc, val) => acc + ((val - mean) / stddev) ** 4, 0) / n - 3;
  const cv = stddev / mean;
  const zScores = data.map(val => (val - mean) / stddev);
  const outliers = data.filter((val, i) => Math.abs(zScores[i]!) > 2);
  return { n, sum, mean, variance, stddev, min, max, range, median, q1, q3, iqr, skewness, kurtosis, cv, outliers };
}
