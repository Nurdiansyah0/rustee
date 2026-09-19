/**
 * Financial Ledger Export Utility (FinRep Pro Personal Accounting)
 * Generates RFC 4180 compliant UTF-8 CSV with Excel-compatible BOM.
 */

export function exportTransactionsToCSV(transactions, options = {}) {
  if (!transactions || transactions.length === 0) {
    throw new Error('Tidak ada data transaksi untuk diekspor.')
  }

  const headers = [
    'Tanggal (UTC)',
    'Deskripsi Transaksi',
    'Tipe Mutasi',
    'Nominal (IDR)',
    'Kategori',
    'Akun / Dompet',
    'Catatan'
  ]

  const rows = transactions.map((tx) => {
    const dateStr = (tx.date || tx.created_at || '').substring(0, 10)
    const desc = escapeCSV(tx.description || 'Transaksi')
    const type = tx.transaction_type === 'income' ? 'Pemasukan' : tx.transaction_type === 'expense' ? 'Pengeluaran' : 'Transfer'
    const amount = String(tx.amount || 0)
    const category = escapeCSV(tx.category_name || tx.category?.name || '-')
    const account = escapeCSV(tx.account_name || tx.account?.name || '-')
    const notes = escapeCSV(tx.notes || '')

    return [dateStr, desc, type, amount, category, account, notes].join(',')
  })

  // Prepend UTF-8 BOM (\uFEFF) for immediate Excel compatibility
  const csvContent = '\uFEFF' + [headers.join(','), ...rows].join('\r\n')
  const blob = new Blob([csvContent], { type: 'text/csv;charset=utf-8;' })
  const url = URL.createObjectURL(blob)

  const filename = options.filename || `FinRep_Buku_Kas_${new Date().toISOString().substring(0, 10).replace(/-/g, '')}.csv`

  const link = document.createElement('a')
  link.setAttribute('href', url)
  link.setAttribute('download', filename)
  document.body.appendChild(link)
  link.click()
  document.body.removeChild(link)
  URL.revokeObjectURL(url)

  return true
}

function escapeCSV(text) {
  if (!text) return '""'
  const escaped = String(text).replace(/"/g, '""')
  return `"${escaped}"`
}
