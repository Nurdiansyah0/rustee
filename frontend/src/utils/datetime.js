/**
 * FinRep Financial Date/Time Policy & Utilities
 * 
 * Single Source of Truth for deterministic financial time handling:
 * Transaction Timestamp (UTC/Canonical) -> Business Timezone (Asia/Jakarta) -> Calendar Date & Month -> Reporting Period
 */

export const BUSINESS_TIMEZONE = 'Asia/Jakarta';

/**
 * Resolves the authoritative financial date from a transaction entity, ISO string, Date, or timestamp.
 * Priority: transaction_date (authoritative ledger occurrence) > date > created_at (audit fallback).
 * 
 * @param {Object|string|Date|number} input 
 * @returns {Date|null} Valid Date object or null
 */
export function resolveFinancialDate(input) {
  if (!input) return null;

  let rawValue = input;
  if (typeof input === 'object' && !(input instanceof Date)) {
    rawValue = input.transaction_date || input.date || input.created_at;
  }

  if (!rawValue) return null;

  if (rawValue instanceof Date) {
    return isNaN(rawValue.getTime()) ? null : rawValue;
  }

  // Handle YYYY-MM-DD plain calendar date without unwanted UTC offset shifts
  if (typeof rawValue === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(rawValue)) {
    const [year, month, day] = rawValue.split('-').map(Number);
    const d = new Date(year, month - 1, day, 12, 0, 0); // Midday local buffer
    return isNaN(d.getTime()) ? null : d;
  }

  const d = new Date(rawValue);
  return isNaN(d.getTime()) ? null : d;
}

/**
 * Extracts canonical "YYYY-MM" reporting period in the business timezone.
 * 
 * @param {Object|string|Date|number} dateInput
 * @param {string} timeZone
 * @returns {string|null} "YYYY-MM" or null
 */
export function getFinancialPeriod(dateInput, timeZone = BUSINESS_TIMEZONE) {
  const d = resolveFinancialDate(dateInput);
  if (!d) return null;

  try {
    const formatter = new Intl.DateTimeFormat('en-CA', {
      timeZone,
      year: 'numeric',
      month: '2-digit'
    });
    return formatter.format(d); // Output: "YYYY-MM"
  } catch {
    const y = d.getFullYear();
    const m = String(d.getMonth() + 1).padStart(2, '0');
    return `${y}-${m}`;
  }
}

/**
 * Determines whether a transaction or date falls into the current reporting period.
 * 
 * @param {Object|string|Date|number} dateInput 
 * @param {string} timeZone 
 * @param {Date} referenceDate 
 * @returns {boolean}
 */
export function isCurrentFinancialMonth(dateInput, timeZone = BUSINESS_TIMEZONE, referenceDate = new Date()) {
  const targetPeriod = getFinancialPeriod(dateInput, timeZone);
  if (!targetPeriod) return false;

  const currentPeriod = getFinancialPeriod(referenceDate, timeZone);
  return targetPeriod === currentPeriod;
}

/**
 * Formats period label (e.g. "September 2026") in the business timezone.
 * 
 * @param {Object|string|Date|number} dateInput 
 * @param {string} timeZone 
 * @param {string} locale 
 * @returns {string}
 */
export function formatFinancialPeriod(dateInput = new Date(), timeZone = BUSINESS_TIMEZONE, locale = 'id-ID') {
  const d = resolveFinancialDate(dateInput) || new Date();
  try {
    return new Intl.DateTimeFormat(locale, {
      timeZone,
      month: 'long',
      year: 'numeric'
    }).format(d);
  } catch {
    return d.toLocaleDateString(locale, { month: 'long', year: 'numeric' });
  }
}

/**
 * Formats a financial date for user interface display.
 * 
 * @param {Object|string|Date|number} dateInput 
 * @param {Object} options 
 * @param {string} [options.timeZone=BUSINESS_TIMEZONE]
 * @param {string} [options.locale='id-ID']
 * @param {boolean} [options.includeTime=false]
 * @param {'short'|'long'|'numeric'} [options.monthFormat='short']
 * @returns {string} Formatted localized date
 */
export function formatFinancialDate(dateInput, options = {}) {
  const {
    timeZone = BUSINESS_TIMEZONE,
    locale = 'id-ID',
    includeTime = false,
    monthFormat = 'short'
  } = options;

  const d = resolveFinancialDate(dateInput);
  if (!d) return '';

  try {
    const formatOptions = {
      timeZone,
      day: 'numeric',
      month: monthFormat,
    };

    if (includeTime) {
      formatOptions.hour = '2-digit';
      formatOptions.minute = '2-digit';
    }

    return new Intl.DateTimeFormat(locale, formatOptions).format(d);
  } catch {
    return d.toLocaleDateString(locale, { day: 'numeric', month: monthFormat });
  }
}
