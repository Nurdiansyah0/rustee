/**
 * Local In-Memory & LocalStorage Mock Provider for FinRep PWA
 * Allows full exploratory testing of all views (Dashboard, Transactions, Analytics, Profile, Modals)
 * when backend API is offline or during UI development.
 */

import {
  resolveFinancialDate,
  isCurrentFinancialMonth,
  formatFinancialPeriod
} from '../utils/datetime.js';

const STORAGE_KEY = 'finrep_mock_database_v2';

function getDefaultDatabase() {
  return {
    users: {
      'demo@example.com': {
        id: 'usr_demo_personal',
        name: 'Budi Santoso',
        display_name: 'Budi Santoso',
        email: 'demo@example.com',
        subscription_tier: 'free',
        permissions: ['accounts.view', 'transactions.create'],
        created_at: '2026-09-01T00:00:00Z'
      },
      'premium@example.com': {
        id: 'usr_demo_pro',
        name: 'Rian Dinata',
        display_name: 'Rian Dinata (Pro)',
        email: 'premium@example.com',
        subscription_tier: 'premium',
        permissions: [
          'accounts.view',
          'transactions.create',
          'analytics.advanced',
          'export.csv',
          'multi.wallets',
          'budget.custom'
        ],
        created_at: '2026-08-15T00:00:00Z'
      }
    },
    accounts: [
      {
        id: 'acc_bca',
        name: 'BCA Prioritas',
        account_type: 'savings',
        balance: 26700000,
        current_balance: 26700000,
        currency: 'IDR',
        color: '#10B981',
        is_archived: false,
        created_at: '2026-09-01T00:00:00Z'
      },
      {
        id: 'acc_mandiri',
        name: 'Mandiri Payroll',
        account_type: 'savings',
        balance: 15250000,
        current_balance: 15250000,
        currency: 'IDR',
        color: '#0284C7',
        is_archived: false,
        created_at: '2026-09-01T00:00:00Z'
      },
      {
        id: 'acc_gopay',
        name: 'GoPay / OVO',
        account_type: 'e-wallet',
        balance: 1350000,
        current_balance: 1350000,
        currency: 'IDR',
        color: '#059669',
        is_archived: false,
        created_at: '2026-09-05T00:00:00Z'
      },
      {
        id: 'acc_cash',
        name: 'Dompet Tunai',
        account_type: 'cash',
        balance: 650000,
        current_balance: 650000,
        currency: 'IDR',
        color: '#F59E0B',
        is_archived: false,
        created_at: '2026-09-01T00:00:00Z'
      }
    ],
    categories: [
      { id: 'cat_gaji', name: 'Gaji & Pendapatan', category_type: 'income', color: '#10B981', icon: 'TrendingUp' },
      { id: 'cat_proyek', name: 'Invoice Proyek', category_type: 'income', color: '#059669', icon: 'FileText' },
      { id: 'cat_makan', name: 'Makanan & Kuliner', category_type: 'expense', color: '#F43F5E', icon: 'Utensils' },
      { id: 'cat_sewa', name: 'Sewa & Tempat Tinggal', category_type: 'expense', color: '#E11D48', icon: 'Home' },
      { id: 'cat_tagihan', name: 'Tagihan & Utilitas', category_type: 'expense', color: '#D97706', icon: 'Zap' },
      { id: 'cat_transport', name: 'Transportasi', category_type: 'expense', color: '#6366F1', icon: 'Car' },
      { id: 'cat_belanja', name: 'Belanja Kebutuhan', category_type: 'expense', color: '#EC4899', icon: 'ShoppingBag' }
    ],
    transactions: [
      {
        id: 'tx_1',
        description: 'Pembayaran Invoice Proyek Q3',
        amount: 14500000,
        transaction_type: 'income',
        account_id: 'acc_bca',
        account_name: 'BCA Prioritas',
        category_id: 'cat_proyek',
        category_name: 'Invoice Proyek',
        transaction_date: '2026-09-25T10:30:00Z',
        created_at: '2026-09-25T10:30:00Z'
      },
      {
        id: 'tx_2',
        description: 'Sewa Kantor & Operasional Bulanan',
        amount: 2800000,
        transaction_type: 'expense',
        account_id: 'acc_bca',
        account_name: 'BCA Prioritas',
        category_id: 'cat_sewa',
        category_name: 'Sewa & Tempat Tinggal',
        transaction_date: '2026-09-12T14:15:00Z',
        created_at: '2026-09-12T14:15:00Z'
      },
      {
        id: 'tx_3',
        description: 'Gaji Pokok & Tunjangan Kerja',
        amount: 8200000,
        transaction_type: 'income',
        account_id: 'acc_mandiri',
        account_name: 'Mandiri Payroll',
        category_id: 'cat_gaji',
        category_name: 'Gaji & Pendapatan',
        transaction_date: '2026-09-01T08:00:00Z',
        created_at: '2026-09-01T08:00:00Z'
      },
      {
        id: 'tx_4',
        description: 'Belanja Bulanan Supermarket',
        amount: 950000,
        transaction_type: 'expense',
        account_id: 'acc_mandiri',
        account_name: 'Mandiri Payroll',
        category_id: 'cat_belanja',
        category_name: 'Belanja Kebutuhan',
        transaction_date: '2026-09-05T19:00:00Z',
        created_at: '2026-09-05T19:00:00Z'
      },
      {
        id: 'tx_5',
        description: 'Topup Pulsa & Paket Data Internet',
        amount: 150000,
        transaction_type: 'expense',
        account_id: 'acc_gopay',
        account_name: 'GoPay / OVO',
        category_id: 'cat_tagihan',
        category_name: 'Tagihan & Utilitas',
        transaction_date: '2026-09-08T11:20:00Z',
        created_at: '2026-09-08T11:20:00Z'
      },
      {
        id: 'tx_6',
        description: 'Makan Siang Tim & Kopi',
        amount: 350000,
        transaction_type: 'expense',
        account_id: 'acc_cash',
        account_name: 'Dompet Tunai',
        category_id: 'cat_makan',
        category_name: 'Makanan & Kuliner',
        transaction_date: '2026-09-15T13:00:00Z',
        created_at: '2026-09-15T13:00:00Z'
      }
    ],
    subscription: {
      status: 'active',
      tier: 'free',
      trial_active: false,
      has_used_trial: false,
      days_remaining: null
    }
  };
}

let memoryDb = null;

function loadDatabase() {
  if (typeof localStorage === 'undefined') {
    if (!memoryDb) memoryDb = getDefaultDatabase();
    return memoryDb;
  }
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      const def = getDefaultDatabase();
      saveDatabase(def);
      return def;
    }
    return JSON.parse(raw);
  } catch {
    return getDefaultDatabase();
  }
}

function saveDatabase(db) {
  if (typeof localStorage !== 'undefined') {
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(db));
    } catch {}
  } else {
    memoryDb = db;
  }
}

export function handleMockApiRequest(path, options = {}) {
  const method = (options.method || 'GET').toUpperCase();
  const db = loadDatabase();
  const url = new URL(path, 'http://localhost');
  const pathname = url.pathname;

  // 1. Auth: Login
  if (pathname === '/api/v1/auth/login' && method === 'POST') {
    const body = JSON.parse(options.body || '{}');
    const email = (body.email || '').toLowerCase().trim();
    const isPro = email.includes('premium') || email.includes('pro');
    const user = isPro
      ? db.users['premium@example.com']
      : (db.users[email] || {
          id: 'usr_' + Math.random().toString(36).substring(2, 9),
          name: email.split('@')[0] || 'Pengguna FinRep',
          display_name: email.split('@')[0] || 'Pengguna FinRep',
          email: email,
          subscription_tier: 'free',
          permissions: ['accounts.view', 'transactions.create'],
          created_at: new Date().toISOString()
        });
    return {
      success: true,
      message: 'Login berhasil (Demo Mode)',
      user
    };
  }

  // 2. Auth: Register
  if (pathname === '/api/v1/auth/register' && method === 'POST') {
    const body = JSON.parse(options.body || '{}');
    const user = {
      id: 'usr_' + Math.random().toString(36).substring(2, 9),
      name: body.display_name || 'Pengguna Baru',
      display_name: body.display_name || 'Pengguna Baru',
      email: body.email,
      subscription_tier: 'free',
      permissions: ['accounts.view', 'transactions.create'],
      created_at: new Date().toISOString()
    };
    return {
      success: true,
      message: 'Pendaftaran berhasil (Demo Mode)',
      user
    };
  }

  // 3. Auth: Logout
  if (pathname === '/api/v1/auth/logout' && method === 'POST') {
    return { success: true };
  }

  // 4. Auth: Me
  if (pathname === '/api/v1/auth/me') {
    let cached = null;
    try {
      cached = JSON.parse(localStorage.getItem('invinite_auth_user') || 'null');
    } catch {}
    if (cached) return cached;
    return db.users['demo@example.com'];
  }

  // 5. Accounts: GET
  if (pathname === '/api/v1/accounts' && method === 'GET') {
    return db.accounts.filter((a) => !a.is_archived);
  }

  // 6. Accounts: POST (Create)
  if (pathname === '/api/v1/accounts' && method === 'POST') {
    const body = JSON.parse(options.body || '{}');
    const newAcc = {
      id: 'acc_' + Math.random().toString(36).substring(2, 9),
      name: body.name || 'Dompet Baru',
      account_type: body.account_type || 'savings',
      balance: parseInt(body.initial_balance || body.balance || 0, 10),
      current_balance: parseInt(body.initial_balance || body.balance || 0, 10),
      currency: 'IDR',
      color: body.color || '#10B981',
      is_archived: false,
      created_at: new Date().toISOString()
    };
    db.accounts.push(newAcc);
    saveDatabase(db);
    return newAcc;
  }

  // 7. Categories: GET
  if (pathname === '/api/v1/categories' && method === 'GET') {
    return db.categories;
  }

  // 8. Transactions: GET
  if (pathname === '/api/v1/transactions' && method === 'GET') {
    const type = url.searchParams.get('transaction_type');
    const accId = url.searchParams.get('account_id');
    const catId = url.searchParams.get('category_id');

    let list = [...db.transactions];
    if (type) list = list.filter((t) => t.transaction_type === type);
    if (accId) list = list.filter((t) => t.account_id === accId);
    if (catId) list = list.filter((t) => t.category_id === catId);

    return {
      data: list,
      total: list.length,
      page: 1,
      per_page: 50
    };
  }

  // 9. Transactions: POST (Create)
  if (pathname === '/api/v1/transactions' && method === 'POST') {
    const body = JSON.parse(options.body || '{}');
    const amount = Math.abs(parseInt(body.amount || 0, 10));
    const txType = body.transaction_type || 'expense';

    const acc = db.accounts.find((a) => a.id === body.account_id) || db.accounts[0];
    const cat = db.categories.find((c) => c.id === body.category_id);

    const newTx = {
      id: 'tx_' + Math.random().toString(36).substring(2, 9),
      description: body.description || 'Transaksi FinRep',
      amount,
      transaction_type: txType,
      account_id: acc?.id || 'acc_bca',
      account_name: acc?.name || 'BCA Prioritas',
      to_account_id: body.to_account_id || null,
      to_account_name: db.accounts.find((a) => a.id === body.to_account_id)?.name || null,
      category_id: cat?.id || null,
      category_name: cat?.name || 'Lainnya',
      transaction_date: body.transaction_date || body.date || new Date().toISOString(),
      date: body.date || body.transaction_date || new Date().toISOString(),
      created_at: new Date().toISOString()
    };

    // Update wallet balance automatically!
    if (txType === 'income' && acc) {
      acc.balance += amount;
      acc.current_balance = acc.balance;
    } else if (txType === 'expense' && acc) {
      acc.balance -= amount;
      acc.current_balance = acc.balance;
    } else if (txType === 'transfer') {
      const toAcc = db.accounts.find((a) => a.id === body.to_account_id);
      if (acc) {
        acc.balance -= amount;
        acc.current_balance = acc.balance;
      }
      if (toAcc) {
        toAcc.balance += amount;
        toAcc.current_balance = toAcc.balance;
      }
    }

    db.transactions.unshift(newTx);
    saveDatabase(db);
    return newTx;
  }

  // 9b. Transactions: DELETE
  if (pathname.startsWith('/api/v1/transactions/') && method === 'DELETE') {
    const id = pathname.split('/').pop();
    const txIdx = db.transactions.findIndex((t) => t.id === id);
    if (txIdx !== -1) {
      const tx = db.transactions[txIdx];
      const acc = db.accounts.find((a) => a.id === tx.account_id);
      if (tx.transaction_type === 'income' && acc) {
        acc.balance -= tx.amount;
        acc.current_balance = acc.balance;
      } else if (tx.transaction_type === 'expense' && acc) {
        acc.balance += tx.amount;
        acc.current_balance = acc.balance;
      } else if (tx.transaction_type === 'transfer') {
        const toAcc = db.accounts.find((a) => a.id === tx.to_account_id);
        if (acc) {
          acc.balance += tx.amount;
          acc.current_balance = acc.balance;
        }
        if (toAcc) {
          toAcc.balance -= tx.amount;
          toAcc.current_balance = toAcc.balance;
        }
      }
      db.transactions.splice(txIdx, 1);
      saveDatabase(db);
      return { message: 'Transaksi berhasil dihapus' };
    }
    return { message: 'Transaksi tidak ditemukan' };
  }

  // 10. Dashboard: GET
  if (pathname === '/api/v1/dashboard') {
    const totalBalance = db.accounts.reduce((sum, a) => sum + (a.is_archived ? 0 : a.balance), 0);
    
    // Exact Asia/Jakarta calendar month filtering for "Arus Kas Bulan Ini"
    const monthlyTransactions = db.transactions.filter((t) => isCurrentFinancialMonth(t));

    const incomeTxs = monthlyTransactions.filter((t) => t.transaction_type === 'income');
    const expenseTxs = monthlyTransactions.filter((t) => t.transaction_type === 'expense');

    const totalIncome = incomeTxs.reduce((sum, t) => sum + t.amount, 0);
    const totalExpenses = expenseTxs.reduce((sum, t) => sum + t.amount, 0);

    // Sorted by date descending for true recent activity
    const sortedAll = [...db.transactions].sort((a, b) => {
      const da = resolveFinancialDate(a)?.getTime() || 0;
      const db_ = resolveFinancialDate(b)?.getTime() || 0;
      return db_ - da;
    });

    return {
      total_balance: totalBalance,
      cash_flow: {
        total_income: totalIncome,
        total_expenses: totalExpenses,
        net_cash_flow: totalIncome - totalExpenses,
        income_count: incomeTxs.length,
        expense_count: expenseTxs.length,
        period_label: formatFinancialPeriod()
      },
      accounts: db.accounts.filter((a) => !a.is_archived),
      recent_transactions: sortedAll.slice(0, 4)
    };
  }

  // 11. Analytics: Basic
  if (pathname === '/api/v1/analytics/basic') {
    const monthlyTransactions = db.transactions.filter((t) => isCurrentFinancialMonth(t));

    const incomeTxs = monthlyTransactions.filter((t) => t.transaction_type === 'income');
    const expenseTxs = monthlyTransactions.filter((t) => t.transaction_type === 'expense');

    const totalIncome = incomeTxs.reduce((sum, t) => sum + t.amount, 0);
    const totalExpenses = expenseTxs.reduce((sum, t) => sum + t.amount, 0);
    const savingsRate = totalIncome > 0 ? Math.round(((totalIncome - totalExpenses) / totalIncome) * 100) : 0;

    return {
      cash_flow: {
        total_income: totalIncome,
        total_expenses: totalExpenses,
        net_cash_flow: totalIncome - totalExpenses,
        income_count: incomeTxs.length,
        expense_count: expenseTxs.length,
        period_label: formatFinancialPeriod()
      },
      savings_rate_percent: Math.max(0, savingsRate)
    };
  }

  // 12. Analytics: Advanced
  if (pathname === '/api/v1/analytics/advanced') {
    return {
      financial_health_score: 88,
      monthly_trend: 'Positif (+8.4% bulan ini)',
      runway_months: 4.5,
      budget_adherence_percent: 92,
      net_worth_growth_percent: 14.2
    };
  }

  // 13. Subscriptions: Status
  if (pathname === '/api/v1/subscription') {
    let cached = null;
    try {
      cached = JSON.parse(localStorage.getItem('invinite_auth_user') || 'null');
    } catch {}
    const isPremium = cached?.subscription_tier === 'premium';
    return {
      status: isPremium ? 'active' : 'free',
      tier: isPremium ? 'premium' : 'free',
      trial_active: false,
      has_used_trial: false,
      days_remaining: null
    };
  }

  // 14. Subscriptions: Trial Activation
  if (pathname === '/api/v1/subscriptions/trial' && method === 'POST') {
    try {
      const cached = JSON.parse(localStorage.getItem('invinite_auth_user') || '{}');
      cached.subscription_tier = 'premium';
      cached.permissions = [
        'accounts.view',
        'transactions.create',
        'analytics.advanced',
        'export.csv',
        'multi.wallets',
        'budget.custom'
      ];
      localStorage.setItem('invinite_auth_user', JSON.stringify(cached));
    } catch {}
    return {
      success: true,
      tier: 'premium',
      trial_active: true,
      days_remaining: 7,
      message: 'Masa uji coba FinRep Pro 7 hari aktif!'
    };
  }

  // Fallback
  return { success: true };
}
