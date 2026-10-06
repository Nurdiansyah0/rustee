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
    },
    tenants: [
      {
        id: 'tnt_personal_default',
        name: 'Ruang Kerja Pribadi',
        slug: 'pribadi',
        status: 'ACTIVE',
        role: 'owner',
        is_default: true,
        is_personal: true,
        business_type: 'personal',
        created_at: '2026-09-01T00:00:00Z',
        updated_at: '2026-09-01T00:00:00Z'
      }
    ],
    warehouses: [
      {
        id: 'wh_main_01',
        code: 'WH-01',
        name: 'Gudang Utama (Jakarta)',
        address: 'Jl. Sudirman Kav. 25, Jakarta Selatan',
        is_default: true,
        created_at: '2026-09-01T00:00:00Z'
      },
      {
        id: 'wh_sec_02',
        code: 'WH-02',
        name: 'Gudang Transit (Bandung)',
        address: 'Jl. Asia Afrika No. 10, Bandung',
        is_default: false,
        created_at: '2026-09-15T00:00:00Z'
      }
    ],
    products: [
      {
        id: 'prd_01',
        sku: 'SKU-000001',
        name: 'Biji Kopi Arabika Gayo 1kg',
        unit: 'KG',
        cost_price: 120000,
        sale_price: 185000,
        reorder_threshold: 10,
        is_active: true,
        created_at: '2026-09-01T00:00:00Z'
      },
      {
        id: 'prd_02',
        sku: 'SKU-000002',
        name: 'Sirup Karamel 750ml',
        unit: 'BTL',
        cost_price: 65000,
        sale_price: 95000,
        reorder_threshold: 5,
        is_active: true,
        created_at: '2026-09-02T00:00:00Z'
      },
      {
        id: 'prd_03',
        sku: 'SKU-000003',
        name: 'Paper Cup Hot 8oz',
        unit: 'SLV',
        cost_price: 25000,
        sale_price: 40000,
        reorder_threshold: 15,
        is_active: true,
        created_at: '2026-09-03T00:00:00Z'
      }
    ],
    stock_items: [
      {
        id: 'stk_01',
        warehouse_id: 'wh_main_01',
        product_id: 'prd_01',
        quantity_on_hand: 45,
        quantity_reserved: 5,
        reorder_threshold: 10,
        bin_location: 'A-01-02',
        average_cost: 120000,
        product_name: 'Biji Kopi Arabika Gayo 1kg',
        product_sku: 'SKU-000001',
        product_unit: 'KG',
        warehouse_name: 'Gudang Utama (Jakarta)',
        updated_at: '2026-09-01T00:00:00Z'
      },
      {
        id: 'stk_02',
        warehouse_id: 'wh_main_01',
        product_id: 'prd_02',
        quantity_on_hand: 4,
        quantity_reserved: 0,
        reorder_threshold: 5,
        bin_location: 'B-02-01',
        average_cost: 65000,
        product_name: 'Sirup Karamel 750ml',
        product_sku: 'SKU-000002',
        product_unit: 'BTL',
        warehouse_name: 'Gudang Utama (Jakarta)',
        updated_at: '2026-09-02T00:00:00Z'
      },
      {
        id: 'stk_03',
        warehouse_id: 'wh_main_01',
        product_id: 'prd_03',
        quantity_on_hand: 60,
        quantity_reserved: 10,
        reorder_threshold: 15,
        bin_location: 'C-01-05',
        average_cost: 25000,
        product_name: 'Paper Cup Hot 8oz',
        product_sku: 'SKU-000003',
        product_unit: 'SLV',
        warehouse_name: 'Gudang Utama (Jakarta)',
        updated_at: '2026-09-03T00:00:00Z'
      },
      {
        id: 'stk_04',
        warehouse_id: 'wh_sec_02',
        product_id: 'prd_01',
        quantity_on_hand: 20,
        quantity_reserved: 0,
        reorder_threshold: 10,
        bin_location: 'T-01-01',
        average_cost: 120000,
        product_name: 'Biji Kopi Arabika Gayo 1kg',
        product_sku: 'SKU-000001',
        product_unit: 'KG',
        warehouse_name: 'Gudang Transit (Bandung)',
        updated_at: '2026-09-15T00:00:00Z'
      }
    ],
    stock_movements: [
      {
        id: 'mov_01',
        movement_type: 'INBOUND',
        product_id: 'prd_01',
        destination_warehouse_id: 'wh_main_01',
        source_warehouse_id: null,
        quantity: 50,
        unit_cost: 120000,
        reference_type: 'PURCHASE_ORDER',
        reference_id: 'po_01',
        batch_number: 'BATCH-2026-09A',
        notes: 'Penerimaan PO awal',
        created_at: '2026-09-01T00:00:00Z'
      }
    ],
    purchase_orders: [
      {
        id: 'po_01',
        po_number: 'PO-2026-000001',
        supplier_name: 'PT Kopi Nusantara Jaya',
        destination_warehouse_id: 'wh_main_01',
        status: 'RECEIVED',
        total_amount: 6000000,
        notes: 'Pengadaan kopi rutin bulanan',
        items: [
          {
            id: 'poi_01',
            purchase_order_id: 'po_01',
            product_id: 'prd_01',
            quantity_ordered: 50,
            quantity_received: 50,
            unit_cost: 120000,
            total_cost: 6000000
          }
        ],
        created_at: '2026-09-01T00:00:00Z',
        updated_at: '2026-09-02T00:00:00Z'
      },
      {
        id: 'po_02',
        po_number: 'PO-2026-000002',
        supplier_name: 'CV Sirup Manis Sentosa',
        destination_warehouse_id: 'wh_main_01',
        status: 'ORDERED',
        total_amount: 1950000,
        notes: 'Restock sirup karamel',
        items: [
          {
            id: 'poi_02',
            purchase_order_id: 'po_02',
            product_id: 'prd_02',
            quantity_ordered: 30,
            quantity_received: 0,
            unit_cost: 65000,
            total_cost: 1950000
          }
        ],
        created_at: '2026-09-20T00:00:00Z',
        updated_at: '2026-09-21T00:00:00Z'
      }
    ]
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
  const searchParams = url.searchParams;

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

  // 3b. Auth: Forgot Password / Recovery
  if ((pathname === '/api/v1/auth/forgot-password' || pathname === '/api/v1/auth/recovery') && method === 'POST') {
    const body = JSON.parse(options.body || '{}');
    const email = (body.email || '').toLowerCase().trim();
    return {
      success: true,
      message: 'Jika email terdaftar di FinRep, tautan pemulihan kata sandi telah dikirimkan ke kotak masuk Anda.',
      email
    };
  }

  // 3c. Auth: Reset Password
  if (pathname === '/api/v1/auth/reset-password' && method === 'POST') {
    return {
      success: true,
      message: 'Kata sandi Anda berhasil diperbarui. Silakan masuk menggunakan kata sandi baru.'
    };
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
    const isTrialing = cached?.trial_active === true || cached?.status === 'trialing';
    const isPremium = cached?.subscription_tier === 'premium' || isTrialing;
    return {
      status: isTrialing ? 'trialing' : (isPremium ? 'active' : 'free'),
      tier: isPremium ? 'premium' : 'free',
      is_premium: isPremium,
      trial_active: isTrialing,
      has_used_trial: isTrialing || cached?.has_used_trial === true,
      days_remaining: isTrialing ? 90 : null
    };
  }

  // 14. Subscriptions: Trial Activation (Supports both /subscriptions/trial and /subscriptions/trial/activate)
  if ((pathname === '/api/v1/subscriptions/trial' || pathname === '/api/v1/subscriptions/trial/activate' || pathname === '/api/v1/subscription/trial') && method === 'POST') {
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
      status: 'trialing',
      trial_active: true,
      days_remaining: 90,
      message: 'Masa uji coba FinRep Pro 3 bulan (90 hari) aktif!'
    };
  }

  // 14b. Subscription: Simulate Payment (§19)
  if (
    (pathname === '/api/v1/subscriptions/simulate-payment' ||
      pathname === '/api/v1/subscription/simulate-payment') &&
    method === 'POST'
  ) {
    const body = JSON.parse(options.body || '{}');
    const planId = body.plan_id || 'premium_monthly';
    try {
      const cached = JSON.parse(localStorage.getItem('invinite_auth_user') || '{}');
      cached.subscription_tier = 'premium';
      cached.permissions = [
        'accounts.view',
        'transactions.create',
        'analytics.advanced',
        'export.csv',
        'export.pdf',
        'multi.wallets',
        'budget.custom'
      ];
      localStorage.setItem('invinite_auth_user', JSON.stringify(cached));
    } catch {}
    return {
      success: true,
      tier: 'premium',
      status: 'active',
      plan_id: planId,
      message: 'Pembayaran simulasi DANA berhasil dikonfirmasi! FinRep Pro aktif.'
    };
  }

  // 15. Cursor Delta Synchronization (§25, §26)
  if (pathname === '/api/v1/sync' && method === 'GET') {
    const cursor = parseInt(searchParams.get('cursor') || '0', 10);
    return {
      cursor: cursor + 1,
      transactions: [],
      accounts: [],
      categories: [],
      has_more: false,
      timestamp: new Date().toISOString()
    };
  }

  // 16. Personalization: GET
  if (pathname === '/api/v1/users/personalization' && method === 'GET') {
    let cachedPers = null;
    try {
      cachedPers = JSON.parse(localStorage.getItem('invinite_user_personalization') || 'null');
    } catch {}
    if (cachedPers) {
      return {
        user_id: 'usr_mock',
        display_name: cachedPers.display_name || 'Pengguna FinRep',
        income_title: cachedPers.income_title || 'Pemasukan',
        expense_title: cachedPers.expense_title || 'Pengeluaran',
        financial_goals: cachedPers.financial_goals || cachedPers.goals || [],
        onboarding_completed: Boolean(cachedPers.onboarding_completed),
        updated_at: new Date().toISOString()
      };
    }
    return {
      user_id: 'usr_mock',
      display_name: 'Pengguna Baru',
      income_title: null,
      expense_title: null,
      financial_goals: [],
      onboarding_completed: false,
      updated_at: null
    };
  }

  // 17. Personalization: PUT
  if (pathname === '/api/v1/users/personalization' && method === 'PUT') {
    const body = JSON.parse(options.body || '{}');
    const updated = {
      user_id: 'usr_mock',
      display_name: body.display_name || 'Pengguna FinRep',
      income_title: body.income_title || 'Pemasukan',
      expense_title: body.expense_title || 'Pengeluaran',
      financial_goals: body.financial_goals || [],
      onboarding_completed: body.onboarding_completed !== undefined ? Boolean(body.onboarding_completed) : true,
      updated_at: new Date().toISOString()
    };
    try {
      localStorage.setItem('invinite_user_personalization', JSON.stringify(updated));
    } catch {}
    return updated;
  }

  // 18. Onboarding: POST
  if (pathname === '/api/v1/users/onboarding' && method === 'POST') {
    const body = JSON.parse(options.body || '{}');
    const pers = {
      user_id: 'usr_mock',
      display_name: body.display_name || 'Pengguna FinRep',
      income_title: body.income_title || 'Gaji',
      expense_title: body.expense_title || 'Makan & Jajan',
      financial_goals: body.financial_goals || [],
      onboarding_completed: true,
      updated_at: new Date().toISOString()
    };
    try {
      localStorage.setItem('invinite_user_personalization', JSON.stringify(pers));
    } catch {}
    return {
      success: true,
      message: 'Onboarding berhasil diselesaikan',
      personalization: pers
    };
  }

  // 19. Tenants: GET (List Workspaces)
  if (pathname === '/api/v1/tenants' && method === 'GET') {
    if (!db.tenants || db.tenants.length === 0) {
      db.tenants = [
        {
          id: 'tnt_personal_default',
          name: 'Ruang Kerja Pribadi',
          slug: 'pribadi',
          status: 'ACTIVE',
          role: 'owner',
          is_default: true,
          is_personal: true,
          business_type: 'personal',
          created_at: new Date().toISOString(),
          updated_at: new Date().toISOString()
        }
      ];
      saveDatabase(db);
    }
    return db.tenants;
  }

  // 20. Tenants: POST (Create Workspace)
  if (pathname === '/api/v1/tenants' && method === 'POST') {
    const body = JSON.parse(options.body || '{}');
    const slug = (body.name || 'bisnis')
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-|-$/g, '') || 'workspace-' + Math.random().toString(36).substring(2, 6);

    const newTenant = {
      id: 'tnt_' + Math.random().toString(36).substring(2, 9),
      name: body.name || 'Ruang Kerja Bisnis',
      slug: body.slug || slug,
      status: 'ACTIVE',
      role: 'owner',
      is_default: false,
      is_personal: false,
      business_type: body.business_type || 'general',
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString()
    };

    if (!db.tenants) db.tenants = [];
    db.tenants.push(newTenant);
    saveDatabase(db);
    return newTenant;
  }

  // 21. Tenants: POST Switch
  if (pathname.startsWith('/api/v1/tenants/') && pathname.endsWith('/switch') && method === 'POST') {
    const parts = pathname.split('/');
    const tenantId = parts[parts.length - 2];
    const tenant = (db.tenants || []).find((t) => t.id === tenantId) || {
      id: tenantId,
      name: 'Ruang Kerja Bisnis',
      slug: 'bisnis',
      role: 'owner',
      status: 'ACTIVE'
    };
    return {
      active_tenant_id: tenant.id,
      name: tenant.name,
      slug: tenant.slug,
      role: tenant.role || 'owner',
      status: 'switched'
    };
  }

  // 22. Tenants: GET Capabilities
  if (pathname.startsWith('/api/v1/tenants/') && pathname.endsWith('/capabilities') && method === 'GET') {
    const parts = pathname.split('/');
    const tenantId = parts[parts.length - 2];
    const tenant = (db.tenants || []).find((t) => t.id === tenantId);
    const bType = tenant?.business_type || (tenant?.is_personal ? 'personal' : 'general');
    const capMap = {
      personal: ['accounts', 'transactions', 'budgets', 'analytics'],
      retail: ['pos', 'inventory', 'purchasing', 'invoicing', 'accounting', 'receivables', 'reports'],
      fnb: ['pos', 'tables', 'kitchen', 'inventory', 'purchasing', 'accounting', 'reports'],
      rental: ['inventory', 'purchasing', 'bookings', 'invoicing', 'receivables', 'accounting'],
      contractor: ['projects', 'milestones', 'invoicing', 'receivables', 'accounting'],
      general: ['inventory', 'purchasing', 'invoicing', 'accounting', 'receivables', 'reports']
    };
    const caps = capMap[bType] || capMap.general;
    return {
      tenant_id: tenantId,
      business_type: bType,
      role: tenant?.role || 'owner',
      capabilities: caps
    };
  }

  // 23. Warehouses: GET & POST
  if (pathname === '/api/v1/warehouses' && method === 'GET') {
    const list = db.warehouses || [];
    return { warehouses: list, count: list.length };
  }

  if (pathname === '/api/v1/warehouses' && method === 'POST') {
    const body = JSON.parse(options.body || '{}');
    const newWh = {
      id: 'wh_' + Math.random().toString(36).substring(2, 9),
      code: body.code || 'WH-' + String((db.warehouses?.length || 0) + 1).padStart(2, '0'),
      name: body.name || 'Gudang Baru',
      address: body.address || null,
      is_default: Boolean(body.is_default),
      created_at: new Date().toISOString()
    };
    if (!db.warehouses) db.warehouses = [];
    if (newWh.is_default) {
      db.warehouses.forEach(w => { w.is_default = false; });
    }
    db.warehouses.push(newWh);
    saveDatabase(db);
    return newWh;
  }

  if (pathname.startsWith('/api/v1/warehouses/') && method === 'GET') {
    const id = pathname.split('/').pop();
    const wh = (db.warehouses || []).find(w => w.id === id);
    if (!wh) throw new Error('Warehouse not found');
    return wh;
  }

  // 24. Products: GET & POST
  if (pathname === '/api/v1/products' && method === 'GET') {
    const list = db.products || [];
    return { products: list, count: list.length };
  }

  if (pathname === '/api/v1/products' && method === 'POST') {
    const body = JSON.parse(options.body || '{}');
    const newPrd = {
      id: 'prd_' + Math.random().toString(36).substring(2, 9),
      sku: 'SKU-' + String((db.products?.length || 0) + 1).padStart(6, '0'),
      name: body.name || 'Produk Baru',
      unit: body.unit || 'PCS',
      cost_price: parseInt(body.cost_price || 0, 10),
      sale_price: parseInt(body.sale_price || 0, 10),
      reorder_threshold: parseInt(body.reorder_threshold || 10, 10),
      is_active: true,
      created_at: new Date().toISOString()
    };
    if (!db.products) db.products = [];
    db.products.push(newPrd);

    // Also seed default stock items for existing warehouses
    if (!db.stock_items) db.stock_items = [];
    (db.warehouses || []).forEach(wh => {
      db.stock_items.push({
        id: 'stk_' + Math.random().toString(36).substring(2, 9),
        warehouse_id: wh.id,
        product_id: newPrd.id,
        quantity_on_hand: 0,
        quantity_reserved: 0,
        reorder_threshold: newPrd.reorder_threshold,
        bin_location: null,
        average_cost: newPrd.cost_price,
        product_name: newPrd.name,
        product_sku: newPrd.sku,
        product_unit: newPrd.unit,
        warehouse_name: wh.name,
        updated_at: new Date().toISOString()
      });
    });

    saveDatabase(db);
    return newPrd;
  }

  if (pathname.startsWith('/api/v1/products/') && method === 'GET') {
    const id = pathname.split('/').pop();
    const prd = (db.products || []).find(p => p.id === id);
    if (!prd) throw new Error('Product not found');
    return prd;
  }

  // 25. Inventory Stock: GET
  if ((pathname === '/api/v1/inventory' || pathname === '/api/v1/inventory/stock') && method === 'GET') {
    let items = db.stock_items || [];
    const whId = searchParams.get('warehouse_id');
    const prdId = searchParams.get('product_id');
    const lowStock = searchParams.get('low_stock');

    if (whId) items = items.filter(i => i.warehouse_id === whId);
    if (prdId) items = items.filter(i => i.product_id === prdId);
    if (lowStock === 'true') items = items.filter(i => i.quantity_on_hand <= i.reorder_threshold);

    return { stock_items: items, count: items.length };
  }

  // 26. Stock Movements: GET & POST
  if (pathname === '/api/v1/inventory/movements' && method === 'GET') {
    let movs = db.stock_movements || [];
    const whId = searchParams.get('warehouse_id');
    const prdId = searchParams.get('product_id');

    if (whId) {
      movs = movs.filter(m => m.source_warehouse_id === whId || m.destination_warehouse_id === whId);
    }
    if (prdId) movs = movs.filter(m => m.product_id === prdId);

    return { movements: movs, count: movs.length };
  }

  if (pathname === '/api/v1/inventory/movements' && method === 'POST') {
    const body = JSON.parse(options.body || '{}');
    const newMov = {
      id: 'mov_' + Math.random().toString(36).substring(2, 9),
      movement_type: body.movement_type,
      product_id: body.product_id,
      source_warehouse_id: body.source_warehouse_id || null,
      destination_warehouse_id: body.destination_warehouse_id || null,
      quantity: parseInt(body.quantity || 0, 10),
      unit_cost: body.unit_cost ? parseInt(body.unit_cost, 10) : null,
      notes: body.notes || null,
      batch_number: body.batch_number || null,
      created_at: new Date().toISOString()
    };
    if (!db.stock_movements) db.stock_movements = [];
    db.stock_movements.unshift(newMov);

    // Update stock item
    const targetWhId = newMov.destination_warehouse_id || newMov.source_warehouse_id;
    const stockItem = (db.stock_items || []).find(s => s.product_id === newMov.product_id && s.warehouse_id === targetWhId);
    if (stockItem) {
      if (newMov.movement_type === 'INBOUND') {
        stockItem.quantity_on_hand += newMov.quantity;
      } else if (newMov.movement_type === 'OUTBOUND') {
        stockItem.quantity_on_hand = Math.max(0, stockItem.quantity_on_hand - newMov.quantity);
      }
      stockItem.updated_at = new Date().toISOString();
    }
    saveDatabase(db);
    return {
      id: newMov.id,
      movement_type: newMov.movement_type,
      product_id: newMov.product_id,
      quantity: newMov.quantity,
      resulting_stock: stockItem ? stockItem.quantity_on_hand : 0,
      created_at: newMov.created_at
    };
  }

  // 27. Stock Transfer: POST
  if ((pathname === '/api/v1/inventory/transfer' || pathname === '/api/v1/inventory/transfers') && method === 'POST') {
    const body = JSON.parse(options.body || '{}');
    const qty = parseInt(body.quantity || 0, 10);
    const srcWhId = body.source_warehouse_id;
    const dstWhId = body.destination_warehouse_id;
    const prdId = body.product_id;

    const srcItem = (db.stock_items || []).find(s => s.product_id === prdId && s.warehouse_id === srcWhId);
    let dstItem = (db.stock_items || []).find(s => s.product_id === prdId && s.warehouse_id === dstWhId);

    if (srcItem) {
      srcItem.quantity_on_hand = Math.max(0, srcItem.quantity_on_hand - qty);
      srcItem.updated_at = new Date().toISOString();
    }

    if (!dstItem && srcItem) {
      dstItem = {
        id: 'stk_' + Math.random().toString(36).substring(2, 9),
        warehouse_id: dstWhId,
        product_id: prdId,
        quantity_on_hand: 0,
        quantity_reserved: 0,
        reorder_threshold: srcItem.reorder_threshold,
        bin_location: null,
        average_cost: srcItem.average_cost,
        product_name: srcItem.product_name,
        product_sku: srcItem.product_sku,
        product_unit: srcItem.product_unit,
        warehouse_name: (db.warehouses || []).find(w => w.id === dstWhId)?.name || 'Gudang Tujuan',
        updated_at: new Date().toISOString()
      };
      if (!db.stock_items) db.stock_items = [];
      db.stock_items.push(dstItem);
    }

    if (dstItem) {
      dstItem.quantity_on_hand += qty;
      dstItem.updated_at = new Date().toISOString();
    }

    const movId = 'mov_' + Math.random().toString(36).substring(2, 9);
    if (!db.stock_movements) db.stock_movements = [];
    db.stock_movements.unshift({
      id: movId,
      movement_type: 'TRANSFER',
      product_id: prdId,
      source_warehouse_id: srcWhId,
      destination_warehouse_id: dstWhId,
      quantity: qty,
      unit_cost: srcItem?.average_cost || 0,
      notes: body.notes || 'Inter-warehouse transfer',
      created_at: new Date().toISOString()
    });

    saveDatabase(db);
    return {
      movement_id: movId,
      source_warehouse_id: srcWhId,
      destination_warehouse_id: dstWhId,
      product_id: prdId,
      quantity: qty,
      status: 'COMPLETED',
      source_remaining: srcItem?.quantity_on_hand || 0,
      destination_total: dstItem?.quantity_on_hand || 0,
      created_at: new Date().toISOString()
    };
  }

  // 28. Stock Adjustment: POST
  if ((pathname === '/api/v1/inventory/adjust' || pathname === '/api/v1/inventory/adjustments') && method === 'POST') {
    const body = JSON.parse(options.body || '{}');
    const whId = body.warehouse_id;
    const prdId = body.product_id;
    const actualQty = parseInt(body.actual_quantity || 0, 10);
    const reason = body.reason || 'Cycle count opname';

    let item = (db.stock_items || []).find(s => s.product_id === prdId && s.warehouse_id === whId);
    const prevQty = item ? item.quantity_on_hand : 0;
    const variance = actualQty - prevQty;

    if (item) {
      item.quantity_on_hand = actualQty;
      item.updated_at = new Date().toISOString();
    }

    const adjId = 'adj_' + Math.random().toString(36).substring(2, 9);
    const adjNum = 'ADJ-2026-' + String(Math.floor(100000 + Math.random() * 900000));

    if (!db.stock_movements) db.stock_movements = [];
    db.stock_movements.unshift({
      id: 'mov_' + Math.random().toString(36).substring(2, 9),
      movement_type: 'ADJUSTMENT',
      product_id: prdId,
      source_warehouse_id: variance < 0 ? whId : null,
      destination_warehouse_id: variance > 0 ? whId : null,
      quantity: Math.abs(variance),
      unit_cost: item?.average_cost || 0,
      reference_type: 'ADJUSTMENT',
      reference_id: adjId,
      notes: reason,
      created_at: new Date().toISOString()
    });

    saveDatabase(db);
    return {
      id: adjId,
      adjustment_number: adjNum,
      warehouse_id: whId,
      product_id: prdId,
      previous_quantity: prevQty,
      actual_quantity: actualQty,
      new_quantity: actualQty,
      variance,
      variance_quantity: variance,
      reason,
      created_at: new Date().toISOString()
    };
  }

  // 29. Purchase Orders: GET & POST
  if (pathname === '/api/v1/purchase-orders' && method === 'GET') {
    const list = db.purchase_orders || [];
    return { purchase_orders: list, count: list.length };
  }

  if (pathname === '/api/v1/purchase-orders' && method === 'POST') {
    const body = JSON.parse(options.body || '{}');
    const items = (body.items || []).map((it, idx) => ({
      id: 'poi_' + Math.random().toString(36).substring(2, 9),
      product_id: it.product_id,
      quantity_ordered: parseInt(it.quantity_ordered || 0, 10),
      quantity_received: 0,
      unit_cost: parseInt(it.unit_cost || 0, 10),
      total_cost: parseInt(it.quantity_ordered || 0, 10) * parseInt(it.unit_cost || 0, 10)
    }));
    const totalAmount = items.reduce((acc, it) => acc + it.total_cost, 0);
    const newPO = {
      id: 'po_' + Math.random().toString(36).substring(2, 9),
      po_number: 'PO-2026-' + String((db.purchase_orders?.length || 0) + 1).padStart(6, '0'),
      supplier_name: body.supplier_name || 'Pemasok',
      destination_warehouse_id: body.destination_warehouse_id,
      status: 'DRAFT',
      total_amount: totalAmount,
      notes: body.notes || null,
      items,
      created_at: new Date().toISOString(),
      updated_at: new Date().toISOString()
    };

    if (!db.purchase_orders) db.purchase_orders = [];
    db.purchase_orders.unshift(newPO);
    saveDatabase(db);
    return newPO;
  }

  if (pathname.startsWith('/api/v1/purchase-orders/') && method === 'GET') {
    const id = pathname.split('/').pop();
    const po = (db.purchase_orders || []).find(p => p.id === id);
    if (!po) throw new Error('Purchase order not found');
    return po;
  }

  // PO Order action
  if (pathname.startsWith('/api/v1/purchase-orders/') && pathname.endsWith('/order') && method === 'POST') {
    const parts = pathname.split('/');
    const poId = parts[parts.length - 2];
    const po = (db.purchase_orders || []).find(p => p.id === poId);
    if (po) {
      po.status = 'ORDERED';
      po.updated_at = new Date().toISOString();
      saveDatabase(db);
      return { id: po.id, status: 'ORDERED', updated_at: po.updated_at };
    }
  }

  // PO Receive action
  if (pathname.startsWith('/api/v1/purchase-orders/') && pathname.endsWith('/receive') && method === 'POST') {
    const parts = pathname.split('/');
    const poId = parts[parts.length - 2];
    const body = JSON.parse(options.body || '{}');
    const po = (db.purchase_orders || []).find(p => p.id === poId);

    if (po) {
      let totalReceivedVal = 0;
      (body.items || []).forEach(recv => {
        const line = po.items.find(it => it.product_id === recv.product_id);
        const qty = parseInt(recv.quantity_received || 0, 10);
        const unitCost = recv.unit_cost ? parseInt(recv.unit_cost, 10) : (line?.unit_cost || 0);

        if (line) {
          line.quantity_received += qty;
          totalReceivedVal += qty * unitCost;
        }

        // Increment stock
        const stk = (db.stock_items || []).find(s => s.product_id === recv.product_id && s.warehouse_id === po.destination_warehouse_id);
        if (stk) {
          // Weighted average cost update: (prev_qty * prev_cost + in_qty * in_cost) / total_qty
          const prevTotalVal = stk.quantity_on_hand * stk.average_cost;
          const incomingVal = qty * unitCost;
          const newTotalQty = stk.quantity_on_hand + qty;
          if (newTotalQty > 0) {
            stk.average_cost = Math.round((prevTotalVal + incomingVal) / newTotalQty);
          }
          stk.quantity_on_hand += qty;
          stk.updated_at = new Date().toISOString();
        }

        // Record inbound movement
        if (!db.stock_movements) db.stock_movements = [];
        db.stock_movements.unshift({
          id: 'mov_' + Math.random().toString(36).substring(2, 9),
          movement_type: 'INBOUND',
          product_id: recv.product_id,
          source_warehouse_id: null,
          destination_warehouse_id: po.destination_warehouse_id,
          quantity: qty,
          unit_cost: unitCost,
          reference_type: 'PURCHASE_ORDER',
          reference_id: po.id,
          batch_number: recv.batch_number || null,
          notes: 'Penerimaan PO ' + po.po_number,
          created_at: new Date().toISOString()
        });
      });

      const allFulfilled = po.items.every(it => it.quantity_received >= it.quantity_ordered);
      po.status = allFulfilled ? 'RECEIVED' : 'PARTIALLY_RECEIVED';
      po.updated_at = new Date().toISOString();
      saveDatabase(db);

      return {
        id: po.id,
        po_number: po.po_number,
        status: po.status,
        total_receipt_value: totalReceivedVal,
        updated_at: po.updated_at
      };
    }
  }

  // PO Cancel action
  if (pathname.startsWith('/api/v1/purchase-orders/') && pathname.endsWith('/cancel') && method === 'POST') {
    const parts = pathname.split('/');
    const poId = parts[parts.length - 2];
    const po = (db.purchase_orders || []).find(p => p.id === poId);
    if (po) {
      po.status = 'CANCELLED';
      po.updated_at = new Date().toISOString();
      saveDatabase(db);
      return { id: po.id, status: 'CANCELLED', updated_at: po.updated_at };
    }
  }

  // Projects & Contractor Operations Mock Handlers (Phase 3)
  if (pathname === '/api/v1/projects' && method === 'GET') {
    if (!db.projects) db.projects = [];
    return { projects: db.projects, count: db.projects.length };
  }

  if (pathname === '/api/v1/projects' && method === 'POST') {
    const body = JSON.parse(options.body || '{}');
    if (!db.projects) db.projects = [];
    const seq = String(db.projects.length + 1).padStart(6, '0');
    const newProj = {
      id: 'prj_' + Math.random().toString(36).substring(2, 9),
      code: `PRJ-2026-${seq}`,
      project_number: `PRJ-2026-${seq}`,
      name: body.name || 'Proyek Baru',
      description: body.description || '',
      status: 'draft',
      billing_model: body.billing_model || 'milestone_based',
      contract_amount: parseInt(body.contract_amount || 0, 10),
      budget_amount: parseInt(body.contract_amount || 0, 10),
      start_date: body.start_date || null,
      end_date: body.end_date || null,
      created_at: new Date().toISOString()
    };
    db.projects.unshift(newProj);
    saveDatabase(db);
    return newProj;
  }

  if (pathname.startsWith('/api/v1/projects/')) {
    const sub = pathname.replace('/api/v1/projects/', '');
    const segments = sub.split('/');
    const projId = segments[0];

    if (segments.length === 1 && method === 'GET') {
      const p = (db.projects || []).find(x => x.id === projId) || {
        id: projId,
        code: 'PRJ-2026-000001',
        name: 'Proyek Konstruksi',
        status: 'in_progress',
        contract_amount: 150000000,
        billing_model: 'milestone_based'
      };
      return p;
    }

    if (segments[1] === 'profitability') {
      return {
        project_id: projId,
        contract_amount: 150000000,
        total_cost: 45000000,
        total_billed: 60000000,
        gross_profit: 15000000,
        margin_percentage: 25.0
      };
    }

    if (segments[1] === 'milestones') return { milestones: [], data: [] };
    if (segments[1] === 'tasks') return { tasks: [], data: [] };
    if (segments[1] === 'materials') return { materials: [], data: [] };
    if (segments[1] === 'labor') return { labor: [], data: [] };
    if (segments[1] === 'expenses') return { expenses: [], data: [] };
    if (segments[1] === 'progress') return { progress_records: [], data: [] };
    if (segments[1] === 'members') return { members: [], data: [] };
  }

  // Fallback
  return { success: true };
}
