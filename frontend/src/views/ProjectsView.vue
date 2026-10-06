<template>
  <div class="space-y-6">
    <!-- Header Row -->
    <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-4">
      <div>
        <div class="flex items-center gap-2">
          <h1 class="text-xl sm:text-2xl font-extrabold text-content-primary tracking-tight">
            Proyek & Kontraktor
          </h1>
          <Badge variant="brand" size="sm">Kontraktor OS</Badge>
        </div>
        <p class="text-xs sm:text-sm text-content-secondary mt-1">
          Pelacakan tahapan pekerjaan, realisasi job costing (material/upah/beban), dan penagihan termin.
        </p>
      </div>

      <div class="flex items-center gap-2">
        <Button
          v-if="selectedProjectId"
          variant="secondary"
          size="sm"
          @click="selectedProjectId = null"
        >
          <template #prefix>
            <ArrowLeft class="w-4 h-4 mr-1.5" />
          </template>
          Kembali ke Daftar
        </Button>
        <Button
          variant="primary"
          size="sm"
          @click="showCreateModal = true"
        >
          <template #prefix>
            <Plus class="w-4 h-4 mr-1.5 stroke-[2.5]" />
          </template>
          Buat Proyek
        </Button>
      </div>
    </div>

    <!-- Error Banner -->
    <div
      v-if="projectsStore.error"
      class="p-4 rounded-xl bg-expense-subtle/40 border border-expense-border text-expense-default text-xs flex items-center justify-between"
    >
      <span>{{ projectsStore.error }}</span>
      <button
        type="button"
        @click="projectsStore.error = null"
        class="text-expense-default hover:underline font-bold ml-2"
      >
        Tutup
      </button>
    </div>

    <!-- Master List View -->
    <div v-if="!selectedProjectId" class="space-y-6">
      <!-- Quick Stats Row -->
      <div class="grid grid-cols-2 sm:grid-cols-4 gap-3">
        <div class="p-4 rounded-2xl bg-surface-card border border-border-subtle shadow-card">
          <div class="text-[11px] font-bold text-content-muted uppercase tracking-wider">Total Proyek</div>
          <div class="text-xl font-extrabold text-content-primary mt-1">{{ projectsStore.totalProjects }}</div>
        </div>
        <div class="p-4 rounded-2xl bg-surface-card border border-border-subtle shadow-card">
          <div class="text-[11px] font-bold text-brand-default uppercase tracking-wider">Aktif Berjalan</div>
          <div class="text-xl font-extrabold text-brand-default mt-1">{{ projectsStore.activeProjects.length }}</div>
        </div>
        <div class="p-4 rounded-2xl bg-surface-card border border-border-subtle shadow-card">
          <div class="text-[11px] font-bold text-income-default uppercase tracking-wider">Selesai</div>
          <div class="text-xl font-extrabold text-income-default mt-1">{{ projectsStore.completedProjects.length }}</div>
        </div>
        <div class="p-4 rounded-2xl bg-surface-card border border-border-subtle shadow-card">
          <div class="text-[11px] font-bold text-content-muted uppercase tracking-wider">Total Nilai Kontrak</div>
          <div class="text-xl font-extrabold text-content-primary mt-1 tabular-nums truncate">
            {{ formatCurrency(projectsStore.overallBudget) }}
          </div>
        </div>
      </div>

      <!-- Filters & Search -->
      <div class="flex flex-col sm:flex-row gap-3">
        <div class="relative flex-1">
          <Search class="w-4 h-4 text-content-muted absolute left-3 top-1/2 -translate-y-1/2" />
          <input
            v-model="searchQuery"
            type="text"
            placeholder="Cari kode proyek, nama, atau lokasi..."
            class="w-full pl-9 pr-3 py-2 rounded-xl border border-border-subtle bg-surface-subtle text-xs text-content-primary placeholder:text-content-muted focus-ring"
          />
        </div>
        <div class="flex items-center gap-2">
          <select
            v-model="statusFilter"
            class="px-3 py-2 rounded-xl border border-border-subtle bg-surface-subtle text-xs text-content-primary font-semibold focus-ring cursor-pointer"
          >
            <option value="">Semua Status</option>
            <option value="draft">Draft</option>
            <option value="in_progress">Dalam Pengerjaan</option>
            <option value="completed">Selesai</option>
            <option value="closed">Ditutup</option>
            <option value="cancelled">Dibatalkan</option>
          </select>
        </div>
      </div>

      <!-- Projects Grid / Cards -->
      <div v-if="projectsStore.loading" class="py-16 text-center text-content-muted text-xs">
        <Loader2 class="w-6 h-6 animate-spin mx-auto mb-2 text-brand-default" />
        Memuat portofolio proyek...
      </div>

      <div v-else-if="filteredProjects.length === 0" class="py-16 text-center rounded-2xl border border-dashed border-border-subtle bg-surface-subtle/50">
        <FolderKanban class="w-10 h-10 text-content-muted mx-auto mb-2" />
        <p class="text-xs font-bold text-content-primary">Belum ada proyek yang sesuai</p>
        <p class="text-[11px] text-content-secondary mt-0.5">Mulai dengan membuat proyek konstruksi atau jasa kontraktor baru.</p>
        <Button variant="primary" size="sm" class="mt-4" @click="showCreateModal = true">
          <Plus class="w-4 h-4 mr-1.5" />
          Tambah Proyek
        </Button>
      </div>

      <div v-else class="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
        <div
          v-for="p in filteredProjects"
          :key="p.id"
          @click="selectProject(p.id)"
          class="p-5 rounded-2xl bg-surface-card border border-border-subtle hover:border-brand-default transition shadow-card cursor-pointer group space-y-4"
        >
          <div class="flex items-start justify-between gap-2">
            <div>
              <div class="text-[10px] font-mono font-bold text-brand-default uppercase tracking-wider">
                {{ p.code }}
              </div>
              <h3 class="text-sm font-bold text-content-primary group-hover:text-brand-default transition mt-0.5 line-clamp-1">
                {{ p.name }}
              </h3>
            </div>
            <span
              :class="[
                'text-[10px] font-extrabold px-2 py-0.5 rounded-full capitalize',
                statusBadgeClass(p.status)
              ]"
            >
              {{ statusLabel(p.status) }}
            </span>
          </div>

          <p class="text-xs text-content-secondary line-clamp-2 min-h-[32px]">
            {{ p.description || 'Tidak ada keterangan tambahan.' }}
          </p>

          <div class="p-3 rounded-xl bg-surface-subtle/60 border border-border-subtle/60 text-xs space-y-1.5">
            <div class="flex items-center justify-between text-content-secondary">
              <span>Nilai Kontrak:</span>
              <span class="font-bold text-content-primary tabular-nums">{{ formatCurrency(p.contract_amount) }}</span>
            </div>
            <div class="flex items-center justify-between text-content-secondary">
              <span>Metode Penagihan:</span>
              <span class="font-semibold text-content-primary capitalize">{{ billingModelLabel(p.billing_model) }}</span>
            </div>
          </div>

          <div class="flex items-center justify-between pt-2 border-t border-border-subtle text-[11px] text-content-muted">
            <span class="flex items-center gap-1">
              <Calendar class="w-3.5 h-3.5" />
              {{ formatDate(p.start_date) }} - {{ formatDate(p.end_date) }}
            </span>
            <span class="text-brand-default font-bold flex items-center gap-1 group-hover:translate-x-0.5 transition">
              Detail <ChevronRight class="w-3.5 h-3.5" />
            </span>
          </div>
        </div>
      </div>
    </div>

    <!-- Project Detail Workspace View -->
    <div v-else class="space-y-6">
      <div v-if="projectsStore.detailLoading" class="py-20 text-center text-content-muted text-xs">
        <Loader2 class="w-6 h-6 animate-spin mx-auto mb-2 text-brand-default" />
        Memuat detail manajemen proyek...
      </div>

      <div v-else-if="projectsStore.currentProject" class="space-y-6">
        <!-- Project Hero Card -->
        <div class="p-6 rounded-2xl bg-surface-card border border-border-subtle shadow-card space-y-4">
          <div class="flex flex-col md:flex-row md:items-center justify-between gap-4">
            <div>
              <div class="flex items-center gap-2">
                <span class="text-xs font-mono font-bold text-brand-default bg-brand-muted px-2 py-0.5 rounded-lg">
                  {{ projectsStore.currentProject.code }}
                </span>
                <span
                  :class="[
                    'text-[10px] font-extrabold px-2.5 py-0.5 rounded-full capitalize',
                    statusBadgeClass(projectsStore.currentProject.status)
                  ]"
                >
                  {{ statusLabel(projectsStore.currentProject.status) }}
                </span>
              </div>
              <h2 class="text-lg sm:text-xl font-extrabold text-content-primary mt-1.5">
                {{ projectsStore.currentProject.name }}
              </h2>
              <p class="text-xs text-content-secondary mt-1">
                {{ projectsStore.currentProject.description || 'Tidak ada deskripsi proyek.' }}
              </p>
            </div>

            <!-- Status Mutation Buttons -->
            <div class="flex items-center gap-2 flex-wrap">
              <Button
                v-if="projectsStore.currentProject.status === 'draft'"
                variant="primary"
                size="sm"
                :disabled="projectsStore.actionLoading"
                @click="updateStatus('in_progress')"
              >
                Mulai Pengerjaan
              </Button>
              <Button
                v-if="projectsStore.currentProject.status === 'in_progress'"
                variant="primary"
                size="sm"
                :disabled="projectsStore.actionLoading"
                @click="updateStatus('completed')"
              >
                Tandai Selesai
              </Button>
              <Button
                v-if="projectsStore.currentProject.status === 'completed'"
                variant="secondary"
                size="sm"
                :disabled="projectsStore.actionLoading"
                @click="updateStatus('closed')"
              >
                Tutup Proyek
              </Button>
            </div>
          </div>

          <!-- Profitability & Job Costing Summary -->
          <div v-if="projectsStore.profitability" class="grid grid-cols-2 sm:grid-cols-4 gap-3 pt-3 border-t border-border-subtle">
            <div class="p-3 rounded-xl bg-surface-subtle">
              <div class="text-[10px] font-bold text-content-muted uppercase">Nilai Kontrak</div>
              <div class="text-sm font-extrabold text-content-primary mt-0.5 tabular-nums">
                {{ formatCurrency(projectsStore.profitability.contract_amount) }}
              </div>
            </div>
            <div class="p-3 rounded-xl bg-surface-subtle">
              <div class="text-[10px] font-bold text-expense-default uppercase">Total Biaya Riil</div>
              <div class="text-sm font-extrabold text-expense-default mt-0.5 tabular-nums">
                {{ formatCurrency(projectsStore.profitability.total_cost) }}
              </div>
            </div>
            <div class="p-3 rounded-xl bg-surface-subtle">
              <div class="text-[10px] font-bold text-brand-default uppercase">Total Ditagihkan</div>
              <div class="text-sm font-extrabold text-brand-default mt-0.5 tabular-nums">
                {{ formatCurrency(projectsStore.profitability.total_billed) }}
              </div>
            </div>
            <div class="p-3 rounded-xl bg-surface-subtle">
              <div class="text-[10px] font-bold uppercase" :class="projectsStore.profitability.gross_profit >= 0 ? 'text-income-default' : 'text-expense-default'">
                Laba Kotor (Margin)
              </div>
              <div class="text-sm font-extrabold mt-0.5 tabular-nums" :class="projectsStore.profitability.gross_profit >= 0 ? 'text-income-default' : 'text-expense-default'">
                {{ formatCurrency(projectsStore.profitability.gross_profit) }} ({{ projectsStore.profitability.margin_percentage.toFixed(1) }}%)
              </div>
            </div>
          </div>
        </div>

        <!-- Detail Tabs Navigation -->
        <div class="flex items-center gap-2 border-b border-border-subtle overflow-x-auto pb-1 scroll-native">
          <button
            v-for="tab in detailTabs"
            :key="tab.id"
            type="button"
            @click="activeDetailTab = tab.id"
            :class="[
              'px-4 py-2 rounded-xl text-xs font-bold transition whitespace-nowrap cursor-pointer',
              activeDetailTab === tab.id
                ? 'bg-brand-default text-white shadow-xs'
                : 'text-content-secondary hover:text-content-primary hover:bg-surface-subtle'
            ]"
          >
            {{ tab.label }}
          </button>
        </div>

        <!-- Tab 1: Milestones & Billing -->
        <div v-if="activeDetailTab === 'milestones'" class="space-y-4">
          <div class="flex items-center justify-between">
            <h3 class="text-sm font-bold text-content-primary">Tahapan / Termin Kontrak</h3>
            <div class="flex items-center gap-2">
              <Button
                v-if="projectsStore.currentProject.billing_model === 'progress_based' || projectsStore.currentProject.billing_model === 'hybrid'"
                variant="secondary"
                size="sm"
                @click="showProgressBillModal = true"
              >
                Tagih Progres Opname
              </Button>
              <Button variant="primary" size="sm" @click="showMilestoneModal = true">
                <Plus class="w-3.5 h-3.5 mr-1" />
                Tambah Termin
              </Button>
            </div>
          </div>

          <div v-if="projectsStore.milestones.length === 0" class="p-8 text-center rounded-xl border border-dashed border-border-subtle text-content-muted text-xs">
            Belum ada termin atau tahapan kerja yang didaftarkan.
          </div>

          <div v-else class="space-y-3">
            <div
              v-for="m in projectsStore.milestones"
              :key="m.id"
              class="p-4 rounded-xl bg-surface-card border border-border-subtle flex flex-col sm:flex-row sm:items-center justify-between gap-3"
            >
              <div class="space-y-1">
                <div class="flex items-center gap-2">
                  <span class="text-xs font-bold text-content-primary">{{ m.name }}</span>
                  <span class="text-[10px] font-mono px-2 py-0.5 rounded-full bg-brand-muted text-brand-default font-bold">
                    {{ m.progress_percentage }}%
                  </span>
                  <span
                    :class="[
                      'text-[10px] font-bold px-2 py-0.5 rounded-full capitalize',
                      statusBadgeClass(m.status)
                    ]"
                  >
                    {{ statusLabel(m.status) }}
                  </span>
                </div>
                <p class="text-xs text-content-secondary">{{ m.description || 'Tidak ada catatan.' }}</p>
                <div class="text-[11px] font-semibold text-content-muted">
                  Nominal: <span class="text-content-primary">{{ formatCurrency(m.amount) }}</span>
                </div>
              </div>

              <div class="flex items-center gap-2">
                <Button
                  v-if="m.status === 'pending'"
                  variant="secondary"
                  size="sm"
                  :disabled="projectsStore.actionLoading"
                  @click="completeMilestone(m.id)"
                >
                  Selesaikan Termin
                </Button>
                <Button
                  v-if="m.status === 'completed'"
                  variant="primary"
                  size="sm"
                  :disabled="projectsStore.actionLoading"
                  @click="billMilestone(m.id)"
                >
                  Terbitkan Faktur
                </Button>
                <span v-if="m.status === 'billed'" class="text-xs font-bold text-income-default flex items-center gap-1">
                  <Check class="w-4 h-4" /> Telah Ditagihkan
                </span>
              </div>
            </div>
          </div>
        </div>

        <!-- Tab 2: Tasks Board -->
        <div v-if="activeDetailTab === 'tasks'" class="space-y-4">
          <div class="flex items-center justify-between">
            <h3 class="text-sm font-bold text-content-primary">Daftar Aktivitas Pekerjaan</h3>
            <Button variant="primary" size="sm" @click="showTaskModal = true">
              <Plus class="w-3.5 h-3.5 mr-1" />
              Tambah Tugas
            </Button>
          </div>

          <div v-if="projectsStore.tasks.length === 0" class="p-8 text-center rounded-xl border border-dashed border-border-subtle text-content-muted text-xs">
            Belum ada tugas lapangan yang dibuat.
          </div>

          <div v-else class="grid grid-cols-1 md:grid-cols-2 gap-3">
            <div
              v-for="t in projectsStore.tasks"
              :key="t.id"
              class="p-4 rounded-xl bg-surface-card border border-border-subtle space-y-2"
            >
              <div class="flex items-start justify-between gap-2">
                <h4 class="text-xs font-bold text-content-primary">{{ t.title }}</h4>
                <select
                  :value="t.status"
                  @change="updateTaskStatus(t.id, $event.target.value)"
                  class="text-[10px] font-bold px-2 py-1 rounded-lg border border-border-subtle bg-surface-subtle cursor-pointer focus-ring"
                >
                  <option value="todo">Belum Mulai</option>
                  <option value="in_progress">Dikerjakan</option>
                  <option value="review">Review</option>
                  <option value="done">Selesai</option>
                </select>
              </div>
              <p class="text-xs text-content-secondary line-clamp-2">{{ t.description || 'Tanpa catatan tugas.' }}</p>
            </div>
          </div>
        </div>

        <!-- Tab 3: Costing & Material Requisition -->
        <div v-if="activeDetailTab === 'costing'" class="space-y-6">
          <!-- Direct Issue Material Action -->
          <div class="p-5 rounded-2xl bg-surface-card border border-border-subtle space-y-4">
            <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
              <div>
                <h3 class="text-sm font-bold text-content-primary">Pengeluaran Material Proyek (Job Costing)</h3>
                <p class="text-xs text-content-secondary mt-0.5">
                  Potong stok gudang langsung ke beban proyek dan jurnal otomatis (Debit 5000 / Kredit 1300).
                </p>
              </div>
              <Button variant="primary" size="sm" @click="showMaterialModal = true">
                <Plus class="w-3.5 h-3.5 mr-1" />
                Keluarkan Material
              </Button>
            </div>

            <!-- Material Issued List -->
            <div v-if="projectsStore.materials.length === 0" class="p-4 text-center text-xs text-content-muted">
              Belum ada material yang dikeluarkan untuk proyek ini.
            </div>
            <div v-else class="overflow-x-auto">
              <table class="w-full text-left text-xs">
                <thead>
                  <tr class="border-b border-border-subtle text-content-muted">
                    <th class="pb-2 font-bold">Produk / Material</th>
                    <th class="pb-2 font-bold">Qty</th>
                    <th class="pb-2 font-bold">Biaya Satuan</th>
                    <th class="pb-2 font-bold">Total Biaya</th>
                    <th class="pb-2 font-bold">Status</th>
                  </tr>
                </thead>
                <tbody class="divide-y divide-border-subtle">
                  <tr v-for="mat in projectsStore.materials" :key="mat.id" class="text-content-primary">
                    <td class="py-2.5 font-semibold">{{ mat.product_name || mat.product_id }}</td>
                    <td class="py-2.5">{{ mat.quantity }}</td>
                    <td class="py-2.5 tabular-nums">{{ formatCurrency(mat.unit_cost) }}</td>
                    <td class="py-2.5 font-bold tabular-nums">{{ formatCurrency(mat.total_cost) }}</td>
                    <td class="py-2.5">
                      <span class="text-[10px] font-extrabold px-2 py-0.5 rounded-full capitalize" :class="statusBadgeClass(mat.status)">
                        {{ mat.status }}
                      </span>
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
          </div>

          <!-- Labor & Subcontractor Costing -->
          <div class="p-5 rounded-2xl bg-surface-card border border-border-subtle space-y-4">
            <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
              <div>
                <h3 class="text-sm font-bold text-content-primary">Upah Kerja & Tenaga Lapangan</h3>
                <p class="text-xs text-content-secondary mt-0.5">Catat jam kerja mandor, tukang, dan subkontraktor.</p>
              </div>
              <Button variant="secondary" size="sm" @click="showLaborModal = true">
                <Plus class="w-3.5 h-3.5 mr-1" />
                Catat Upah
              </Button>
            </div>

            <div v-if="projectsStore.labor.length === 0" class="p-4 text-center text-xs text-content-muted">
              Belum ada pencatatan jam kerja.
            </div>
            <div v-else class="overflow-x-auto">
              <table class="w-full text-left text-xs">
                <thead>
                  <tr class="border-b border-border-subtle text-content-muted">
                    <th class="pb-2 font-bold">Pekerja</th>
                    <th class="pb-2 font-bold">Peran</th>
                    <th class="pb-2 font-bold">Jam</th>
                    <th class="pb-2 font-bold">Tarif / Jam</th>
                    <th class="pb-2 font-bold">Total Upah</th>
                  </tr>
                </thead>
                <tbody class="divide-y divide-border-subtle">
                  <tr v-for="lab in projectsStore.labor" :key="lab.id" class="text-content-primary">
                    <td class="py-2.5 font-semibold">{{ lab.worker_name }}</td>
                    <td class="py-2.5 capitalize">{{ lab.role_description || '-' }}</td>
                    <td class="py-2.5">{{ lab.hours_worked }} jam</td>
                    <td class="py-2.5 tabular-nums">{{ formatCurrency(lab.hourly_rate) }}</td>
                    <td class="py-2.5 font-bold tabular-nums">{{ formatCurrency(lab.total_cost) }}</td>
                  </tr>
                </tbody>
              </table>
            </div>
          </div>

          <!-- Expenses Costing -->
          <div class="p-5 rounded-2xl bg-surface-card border border-border-subtle space-y-4">
            <div class="flex flex-col sm:flex-row sm:items-center justify-between gap-3">
              <div>
                <h3 class="text-sm font-bold text-content-primary">Beban Operasional Lapangan</h3>
                <p class="text-xs text-content-secondary mt-0.5">Sewa alat berat, bahan bakar, akomodasi, dan perizinan.</p>
              </div>
              <Button variant="secondary" size="sm" @click="showExpenseModal = true">
                <Plus class="w-3.5 h-3.5 mr-1" />
                Catat Beban
              </Button>
            </div>

            <div v-if="projectsStore.expenses.length === 0" class="p-4 text-center text-xs text-content-muted">
              Belum ada beban operasional tercatat.
            </div>
            <div v-else class="overflow-x-auto">
              <table class="w-full text-left text-xs">
                <thead>
                  <tr class="border-b border-border-subtle text-content-muted">
                    <th class="pb-2 font-bold">Kategori</th>
                    <th class="pb-2 font-bold">Keterangan</th>
                    <th class="pb-2 font-bold">Nominal</th>
                  </tr>
                </thead>
                <tbody class="divide-y divide-border-subtle">
                  <tr v-for="exp in projectsStore.expenses" :key="exp.id" class="text-content-primary">
                    <td class="py-2.5 font-semibold capitalize">{{ exp.category }}</td>
                    <td class="py-2.5">{{ exp.description || '-' }}</td>
                    <td class="py-2.5 font-bold tabular-nums">{{ formatCurrency(exp.amount) }}</td>
                  </tr>
                </tbody>
              </table>
            </div>
          </div>
        </div>
      </div>
    </div>

    <!-- Modal: Buat Proyek Baru -->
    <ModalSheet v-model="showCreateModal" title="Buat Proyek Baru">
      <form @submit.prevent="handleCreateProject" class="space-y-4 text-xs">
        <div>
          <label class="block font-bold text-content-primary mb-1">Nama Proyek *</label>
          <input
            v-model="newProject.name"
            required
            type="text"
            placeholder="Contoh: Pembangunan Ruko Graha Mas"
            class="w-full px-3 py-2 rounded-xl border border-border-subtle bg-surface-subtle text-content-primary focus-ring"
          />
        </div>
        <div>
          <label class="block font-bold text-content-primary mb-1">Deskripsi Proyek</label>
          <textarea
            v-model="newProject.description"
            rows="2"
            placeholder="Ruang lingkup pekerjaan dan spesifikasi"
            class="w-full px-3 py-2 rounded-xl border border-border-subtle bg-surface-subtle text-content-primary focus-ring"
          ></textarea>
        </div>
        <div class="grid grid-cols-2 gap-3">
          <div>
            <label class="block font-bold text-content-primary mb-1">Nilai Kontrak (Rp) *</label>
            <input
              v-model.number="newProject.contract_amount"
              required
              min="0"
              type="number"
              class="w-full px-3 py-2 rounded-xl border border-border-subtle bg-surface-subtle text-content-primary focus-ring"
            />
          </div>
          <div>
            <label class="block font-bold text-content-primary mb-1">Metode Penagihan *</label>
            <select
              v-model="newProject.billing_model"
              class="w-full px-3 py-2 rounded-xl border border-border-subtle bg-surface-subtle text-content-primary focus-ring cursor-pointer"
            >
              <option value="milestone_based">Termin / Milestone</option>
              <option value="progress_based">Progres Fisik (%)</option>
              <option value="time_and_materials">Time & Materials</option>
              <option value="hybrid">Hybrid</option>
            </select>
          </div>
        </div>
        <div class="grid grid-cols-2 gap-3">
          <div>
            <label class="block font-bold text-content-primary mb-1">Tanggal Mulai</label>
            <input
              v-model="newProject.start_date"
              type="date"
              class="w-full px-3 py-2 rounded-xl border border-border-subtle bg-surface-subtle text-content-primary focus-ring"
            />
          </div>
          <div>
            <label class="block font-bold text-content-primary mb-1">Target Selesai</label>
            <input
              v-model="newProject.end_date"
              type="date"
              class="w-full px-3 py-2 rounded-xl border border-border-subtle bg-surface-subtle text-content-primary focus-ring"
            />
          </div>
        </div>
        <div class="flex justify-end gap-2 pt-3 border-t border-border-subtle">
          <Button variant="secondary" size="sm" type="button" @click="showCreateModal = false">Batal</Button>
          <Button variant="primary" size="sm" type="submit" :disabled="projectsStore.actionLoading">
            {{ projectsStore.actionLoading ? 'Menyimpan...' : 'Simpan Proyek' }}
          </Button>
        </div>
      </form>
    </ModalSheet>

    <!-- Modal: Tambah Termin / Milestone -->
    <ModalSheet v-model="showMilestoneModal" title="Tambah Termin Kontrak">
      <form @submit.prevent="handleCreateMilestone" class="space-y-4 text-xs">
        <div>
          <label class="block font-bold text-content-primary mb-1">Nama Termin *</label>
          <input
            v-model="newMilestone.name"
            required
            type="text"
            placeholder="Contoh: DP 20% Pondasi Selesai"
            class="w-full px-3 py-2 rounded-xl border border-border-subtle bg-surface-subtle text-content-primary focus-ring"
          />
        </div>
        <div class="grid grid-cols-2 gap-3">
          <div>
            <label class="block font-bold text-content-primary mb-1">Persentase (%) *</label>
            <input
              v-model.number="newMilestone.progress_percentage"
              required
              min="0"
              max="100"
              type="number"
              class="w-full px-3 py-2 rounded-xl border border-border-subtle bg-surface-subtle text-content-primary focus-ring"
            />
          </div>
          <div>
            <label class="block font-bold text-content-primary mb-1">Nominal (Rp) *</label>
            <input
              v-model.number="newMilestone.amount"
              required
              min="0"
              type="number"
              class="w-full px-3 py-2 rounded-xl border border-border-subtle bg-surface-subtle text-content-primary focus-ring"
            />
          </div>
        </div>
        <div class="flex justify-end gap-2 pt-3 border-t border-border-subtle">
          <Button variant="secondary" size="sm" type="button" @click="showMilestoneModal = false">Batal</Button>
          <Button variant="primary" size="sm" type="submit" :disabled="projectsStore.actionLoading">Simpan</Button>
        </div>
      </form>
    </ModalSheet>

    <!-- Modal: Pengeluaran Material Langsung -->
    <ModalSheet v-model="showMaterialModal" title="Keluarkan Material dari Gudang">
      <form @submit.prevent="handleIssueMaterial" class="space-y-4 text-xs">
        <div>
          <label class="block font-bold text-content-primary mb-1">Product ID / SKU *</label>
          <input
            v-model="newMaterial.product_id"
            required
            type="text"
            placeholder="UUID Produk atau SKU"
            class="w-full px-3 py-2 rounded-xl border border-border-subtle bg-surface-subtle text-content-primary focus-ring font-mono"
          />
        </div>
        <div>
          <label class="block font-bold text-content-primary mb-1">Gudang Asal (Warehouse ID) *</label>
          <input
            v-model="newMaterial.warehouse_id"
            required
            type="text"
            placeholder="UUID Lokasi Gudang"
            class="w-full px-3 py-2 rounded-xl border border-border-subtle bg-surface-subtle text-content-primary focus-ring font-mono"
          />
        </div>
        <div class="grid grid-cols-2 gap-3">
          <div>
            <label class="block font-bold text-content-primary mb-1">Jumlah (Qty) *</label>
            <input
              v-model.number="newMaterial.quantity"
              required
              min="1"
              type="number"
              class="w-full px-3 py-2 rounded-xl border border-border-subtle bg-surface-subtle text-content-primary focus-ring"
            />
          </div>
          <div>
            <label class="block font-bold text-content-primary mb-1">Biaya Satuan (Rp)</label>
            <input
              v-model.number="newMaterial.unit_cost"
              min="0"
              type="number"
              placeholder="Otomatis dari HPP gudang jika 0"
              class="w-full px-3 py-2 rounded-xl border border-border-subtle bg-surface-subtle text-content-primary focus-ring"
            />
          </div>
        </div>
        <div class="flex justify-end gap-2 pt-3 border-t border-border-subtle">
          <Button variant="secondary" size="sm" type="button" @click="showMaterialModal = false">Batal</Button>
          <Button variant="primary" size="sm" type="submit" :disabled="projectsStore.actionLoading">Keluarkan Stok</Button>
        </div>
      </form>
    </ModalSheet>

    <!-- Modal: Penagihan Progres Opname -->
    <ModalSheet v-model="showProgressBillModal" title="Tagih Progres Opname (%)">
      <form @submit.prevent="handleBillProgress" class="space-y-4 text-xs">
        <div>
          <label class="block font-bold text-content-primary mb-1">Progres Kumulatif Saat Ini (%) *</label>
          <input
            v-model.number="progressBill.progress_percentage"
            required
            min="1"
            max="100"
            type="number"
            class="w-full px-3 py-2 rounded-xl border border-border-subtle bg-surface-subtle text-content-primary focus-ring"
          />
        </div>
        <div>
          <label class="block font-bold text-content-primary mb-1">Catatan Berita Acara (BAP)</label>
          <textarea
            v-model="progressBill.description"
            rows="2"
            placeholder="Keterangan opname lapangan"
            class="w-full px-3 py-2 rounded-xl border border-border-subtle bg-surface-subtle text-content-primary focus-ring"
          ></textarea>
        </div>
        <div class="flex justify-end gap-2 pt-3 border-t border-border-subtle">
          <Button variant="secondary" size="sm" type="button" @click="showProgressBillModal = false">Batal</Button>
          <Button variant="primary" size="sm" type="submit" :disabled="projectsStore.actionLoading">Terbitkan Faktur</Button>
        </div>
      </form>
    </ModalSheet>
  </div>
</template>

<script setup>
import { ref, computed, onMounted } from 'vue'
import { useProjectsStore } from '@/stores/projects'
import { Button, Badge, ModalSheet } from '@/components/ui'
import {
  FolderKanban,
  Plus,
  Search,
  Calendar,
  ChevronRight,
  ArrowLeft,
  Loader2,
  Check
} from 'lucide-vue-next'

const projectsStore = useProjectsStore()

const selectedProjectId = ref(null)
const searchQuery = ref('')
const statusFilter = ref('')
const activeDetailTab = ref('milestones')

// Modals
const showCreateModal = ref(false)
const showMilestoneModal = ref(false)
const showTaskModal = ref(false)
const showMaterialModal = ref(false)
const showLaborModal = ref(false)
const showExpenseModal = ref(false)
const showProgressBillModal = ref(false)

const detailTabs = [
  { id: 'milestones', label: 'Termin & Penagihan' },
  { id: 'costing', label: 'Biaya Riil (Job Costing)' },
  { id: 'tasks', label: 'Aktivitas Pekerjaan' }
]

// Forms
const newProject = ref({
  name: '',
  description: '',
  contract_amount: 0,
  billing_model: 'milestone_based',
  start_date: '',
  end_date: ''
})

const newMilestone = ref({
  name: '',
  progress_percentage: 0,
  amount: 0
})

const newMaterial = ref({
  product_id: '',
  warehouse_id: '',
  quantity: 1,
  unit_cost: 0
})

const progressBill = ref({
  progress_percentage: 0,
  description: ''
})

onMounted(() => {
  projectsStore.fetchProjects()
})

const filteredProjects = computed(() => {
  const list = Array.isArray(projectsStore.projects) ? projectsStore.projects : []
  return list.filter((p) => {
    const matchesSearch =
      !searchQuery.value ||
      p.name?.toLowerCase().includes(searchQuery.value.toLowerCase()) ||
      p.code?.toLowerCase().includes(searchQuery.value.toLowerCase())
    const matchesStatus = !statusFilter.value || p.status === statusFilter.value
    return matchesSearch && matchesStatus
  })
})

function selectProject(id) {
  selectedProjectId.value = id
  projectsStore.fetchProjectDetail(id)
}

async function handleCreateProject() {
  await projectsStore.createProject(newProject.value)
  showCreateModal.value = false
  newProject.value = {
    name: '',
    description: '',
    contract_amount: 0,
    billing_model: 'milestone_based',
    start_date: '',
    end_date: ''
  }
}

async function updateStatus(status) {
  if (!selectedProjectId.value) return
  await projectsStore.updateProjectStatus(selectedProjectId.value, status)
}

async function handleCreateMilestone() {
  if (!selectedProjectId.value) return
  await projectsStore.createMilestone(selectedProjectId.value, newMilestone.value)
  showMilestoneModal.value = false
  newMilestone.value = { name: '', progress_percentage: 0, amount: 0 }
}

async function completeMilestone(milestoneId) {
  if (!selectedProjectId.value) return
  await projectsStore.completeMilestone(selectedProjectId.value, milestoneId)
}

async function billMilestone(milestoneId) {
  if (!selectedProjectId.value) return
  await projectsStore.billMilestone(selectedProjectId.value, milestoneId)
}

async function updateTaskStatus(taskId, status) {
  if (!selectedProjectId.value) return
  await projectsStore.updateTaskStatus(selectedProjectId.value, taskId, status)
}

async function handleIssueMaterial() {
  if (!selectedProjectId.value) return
  await projectsStore.directIssueMaterial(selectedProjectId.value, newMaterial.value)
  showMaterialModal.value = false
  newMaterial.value = { product_id: '', warehouse_id: '', quantity: 1, unit_cost: 0 }
}

async function handleBillProgress() {
  if (!selectedProjectId.value) return
  await projectsStore.billProgress(selectedProjectId.value, progressBill.value)
  showProgressBillModal.value = false
  progressBill.value = { progress_percentage: 0, description: '' }
}

function formatCurrency(amount) {
  return new Intl.NumberFormat('id-ID', {
    style: 'currency',
    currency: 'IDR',
    maximumFractionDigits: 0
  }).format(amount || 0)
}

function formatDate(d) {
  if (!d) return '-'
  return new Date(d).toLocaleDateString('id-ID', { day: 'numeric', month: 'short', year: 'numeric' })
}

function statusBadgeClass(status) {
  switch (status) {
    case 'in_progress':
    case 'active':
      return 'bg-brand-muted text-brand-default'
    case 'completed':
    case 'done':
    case 'billed':
      return 'bg-income-subtle text-income-default'
    case 'cancelled':
      return 'bg-expense-subtle text-expense-default'
    default:
      return 'bg-surface-sunken text-content-secondary'
  }
}

function statusLabel(status) {
  const map = {
    draft: 'Draft',
    in_progress: 'Dikerjakan',
    active: 'Aktif',
    completed: 'Selesai',
    closed: 'Ditutup',
    cancelled: 'Dibatalkan',
    pending: 'Tertunda',
    billed: 'Ditagihkan',
    todo: 'Belum Mulai',
    review: 'Review',
    done: 'Selesai'
  }
  return map[status] || status
}

function billingModelLabel(model) {
  const map = {
    milestone_based: 'Termin / Milestone',
    progress_based: 'Progres Fisik (%)',
    time_and_materials: 'Time & Materials',
    hybrid: 'Hybrid'
  }
  return map[model] || model
}
</script>
