/**
 * Localized Indonesian Rupiah (id-ID) currency formatter & integer math utilities.
 * Operates strictly on integer minor units (i64 Rupiah) to eliminate floating-point precision errors.
 * Complies with Master Specification v3.1.0 §15.
 */

/**
 * Formats an integer amount as localized Indonesian Rupiah (e.g. "Rp 50.000").
 * 
 * @param {number|string|bigint} amount - Monetary amount in integer Rupiah
 * @returns {string} Formatted IDR string
 */
export function formatIDR(amount) {
  if (amount === undefined || amount === null || amount === '') return 'Rp 0';
  const num = typeof amount === 'number' 
    ? Math.trunc(amount) 
    : parseInt(String(amount).replace(/[^0-9-]/g, ''), 10) || 0;
  
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

/**
 * Formats an integer amount into compact Indonesian representation (e.g. "50 rb", "1.5 jt", "2.5 M").
 * 
 * @param {number|string|bigint} amount - Monetary amount in integer Rupiah
 * @returns {string} Compact formatted string
 */
export function formatCompactIDR(amount) {
  if (amount === undefined || amount === null || amount === '') return '0';
  const num = typeof amount === 'number' 
    ? Math.trunc(amount) 
    : parseInt(String(amount).replace(/[^0-9-]/g, ''), 10) || 0;
    
  const absNum = Math.abs(num);
  const sign = num < 0 ? '-' : '';

  if (absNum >= 1_000_000_000) {
    const val = (absNum / 1_000_000_000).toFixed(1).replace(/\.0$/, '');
    return `${sign}${val} M`;
  }
  if (absNum >= 1_000_000) {
    const val = (absNum / 1_000_000).toFixed(1).replace(/\.0$/, '');
    return `${sign}${val} jt`;
  }
  if (absNum >= 1_000) {
    const val = (absNum / 1_000).toFixed(0);
    return `${sign}${val} rb`;
  }
  return `${sign}${absNum}`;
}

/**
 * Parses user input string or currency formatted text back into safe integer Rupiah.
 * 
 * @param {string|number} input 
 * @returns {number} Integer Rupiah value
 */
export function parseIDR(input) {
  if (typeof input === 'number') return Math.trunc(input);
  if (!input) return 0;
  const cleaned = String(input).replace(/[^0-9-]/g, '');
  const parsed = parseInt(cleaned, 10);
  return isNaN(parsed) ? 0 : parsed;
}

/**
 * Formats a plain integer number with dot thousands separator (e.g. "50.000").
 * 
 * @param {number|string} amount 
 * @returns {string}
 */
export function formatIntegerRupiah(amount) {
  const num = parseIDR(amount);
  return new Intl.NumberFormat('id-ID').format(num);
}
