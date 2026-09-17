/**
 * FinRep / Invinite Financial Date/Time Policy
 * 
 * Complies with Master Specification v3.1.0 §10:
 * Server Persists in UTC -> API Transports in UTC -> Client Presents in Asia/Jakarta (WIB)
 */

export const BUSINESS_TIMEZONE = 'Asia/Jakarta';

/**
 * Resolves authoritative financial date from a transaction entity, ISO string, Date, or timestamp.
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

  // Handle YYYY-MM-DD plain calendar date without unintended UTC offset shifts
  if (typeof rawValue === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(rawValue)) {
    const [year, month, day] = rawValue.split('-').map(Number);
    const d = new Date(year, month - 1, day, 12, 0, 0); // Midday local buffer
    return isNaN(d.getTime()) ? null : d;
  }

  const d = new Date(rawValue);
  return isNaN(d.getTime()) ? null : d;
}

/**
 * Extracts canonical "YYYY-MM" reporting period in the Asia/Jakarta business timezone.
 * 
 * @param {Object|string|Date|number} dateInput
 * @param {string} [timeZone=BUSINESS_TIMEZONE]
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
    return formatter.format(d); // e.g. "2026-09"
  } catch {
    const y = d.getFullYear();
    const m = String(d.getMonth() + 1).padStart(2, '0');
    return `${y}-${m}`;
  }
}

/**
 * Checks whether a transaction or date falls into the current reporting period.
 * 
 * @param {Object|string|Date|number} dateInput 
 * @param {string} [timeZone=BUSINESS_TIMEZONE] 
 * @param {Date} [referenceDate=new Date()] 
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
 * @param {Object|string|Date|number} [dateInput=new Date()] 
 * @param {string} [timeZone=BUSINESS_TIMEZONE] 
 * @param {string} [locale='id-ID'] 
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
 * Formats a UTC timestamp or financial date for user interface display in Asia/Jakarta timezone.
 * 
 * @param {Object|string|Date|number} dateInput 
 * @param {Object} [options={}] 
 * @param {string} [options.timeZone=BUSINESS_TIMEZONE]
 * @param {string} [options.locale='id-ID']
 * @param {boolean} [options.includeTime=false]
 * @param {'short'|'long'|'numeric'} [options.monthFormat='short']
 * @returns {string} Formatted localized date in WIB
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

/**
 * Formats only the time component in Asia/Jakarta (e.g. "14:30 WIB").
 * 
 * @param {Object|string|Date|number} dateInput 
 * @param {string} [timeZone=BUSINESS_TIMEZONE] 
 * @returns {string}
 */
export function formatJakartaTime(dateInput, timeZone = BUSINESS_TIMEZONE) {
  const d = resolveFinancialDate(dateInput);
  if (!d) return '';

  try {
    const timeStr = new Intl.DateTimeFormat('id-ID', {
      timeZone,
      hour: '2-digit',
      minute: '2-digit',
      hour12: false
    }).format(d);
    return `${timeStr} WIB`;
  } catch {
    return '';
  }
}

/**
 * Converts a date to an ISO 8601 string adjusted for Asia/Jakarta presentation.
 * 
 * @param {Date|string|number} dateInput 
 * @returns {string}
 */
export function toJakartaISOString(dateInput) {
  const d = resolveFinancialDate(dateInput) || new Date();
  return d.toISOString();
}
