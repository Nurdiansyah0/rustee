/**
 * Localized Indonesian Rupiah (id-ID) currency formatter.
 * Operates purely on integer minor units (i64 Rupiah) to eliminate floating-point precision errors.
 */

export function formatIDR(amount) {
  if (amount === undefined || amount === null) return 'Rp 0';
  const num = typeof amount === 'number' ? Math.round(amount) : parseInt(amount, 10) || 0;
  const isNegative = num < 0;
  const absNum = Math.abs(num);

  const formatted = new Intl.NumberFormat('id-ID', {
    style: 'currency',
    currency: 'IDR',
    minimumFractionDigits: 0,
    maximumFractionDigits: 0,
  }).format(absNum);

  return isNegative ? `-${formatted}` : formatted;
}

export function formatCompactIDR(amount) {
  if (amount === undefined || amount === null) return '0';
  const num = typeof amount === 'number' ? Math.round(amount) : parseInt(amount, 10) || 0;
  const absNum = Math.abs(num);
  const sign = num < 0 ? '-' : '';

  if (absNum >= 1_000_000_000) {
    return `${sign}${(absNum / 1_000_000_000).toFixed(1)} M`;
  }
  if (absNum >= 1_000_000) {
    return `${sign}${(absNum / 1_000_000).toFixed(1)} jt`;
  }
  if (absNum >= 1_000) {
    return `${sign}${(absNum / 1_000).toFixed(0)} rb`;
  }
  return `${sign}${absNum}`;
}
