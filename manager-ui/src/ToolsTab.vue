<template>
  <template v-if="detail">
    <div class="subhead">
      <button class="back" @click="detail = null">‹</button>
      <span>{{ detail.name }}</span>
      <span class="sp" style="flex:1"></span>
      <span class="dot" :class="detail.state === 'ready' ? 'g' : detail.in_progress ? 'b' : 'z'" style="margin-right:2px"></span>
    </div>
    <div class="body">
      <div v-if="detail.loading" class="empty"><span class="spin"></span></div>
      <div v-else-if="detail.error" class="empty">{{ detail.error }}</div>
      <template v-else>
        <div class="dependency-intro">
          <div class="dependency-title">{{ (detail.tasks || []).join(' · ') }}</div>
          <div v-if="detail.state === 'ready'">Downloaded and ready in Stimma. Other versions of this model can be added below.</div>
          <div v-else>Download the checkpoint and the components it needs. The tool appears in Stimma when the download finishes.</div>
        </div>
        <div class="grp">
          <h4>Files <span class="n">{{ detail.state === 'ready' ? '' : (detail.missing ? `${detail.missing} missing` : '') }}</span></h4>
          <div v-for="f in detail.files" :key="f.filename" class="li" style="min-height:32px;padding:6px 0">
            <span class="dot" :class="f.installed ? 'g' : 'z'"></span>
            <div class="t">
              <div class="a mono" :title="f.filename">{{ f.filename }}</div>
              <div class="b">{{ f.role }}{{ f.installed ? '' : ' · missing' }}</div>
            </div>
            <div class="r mono">{{ f.size ? fmtBytes(f.size) : '' }}</div>
          </div>
        </div>
        <div class="grp" v-if="otherVariants.length">
          <h4>Other versions <span class="n">{{ otherVariants.length }}</span></h4>
          <div v-for="v in otherVariants" :key="v.filename" class="li" :class="{ dim: !v.installed }" style="min-height:32px;padding:6px 0">
            <span class="dot" :class="v.installed ? 'g' : 'z'"></span>
            <div class="t">
              <div class="a">{{ v.name }}</div>
              <div class="b mono">{{ v.filename }}</div>
            </div>
            <div class="r">
              <span v-if="v.installed" class="mono">{{ v.size ? fmtBytes(v.size) : 'ready' }}</span>
              <button v-else class="btn sm" @click="openDetail({ id: detail.id, name: detail.name, state: detail.state }, v.filename)">Details</button>
            </div>
          </div>
        </div>
      </template>
    </div>
    <div class="foot" v-if="!detail.loading && !detail.error && detail.missing">
      <span>{{ missingSummary }}</span>
      <span class="sp"></span>
      <button v-if="detail.in_progress" style="color:var(--accent-hi)" @click="detail = null; $emit('activity')">Activity</button>
      <button v-else class="btn sm primary" :disabled="busy" @click="openPlanFromDetail">Get ready</button>
    </div>
    <div class="foot" v-else-if="!detail.loading && !detail.error"><span class="mono">{{ detail.file }}</span></div>
  </template>
  <template v-else>
  <div class="pills">
    <button v-for="f in filters" :key="f.id" class="pill" :class="{ on: filter === f.id }" @click="filter = f.id">{{ f.label }}<span class="n">{{ f.count }}</span></button>
  </div>
  <div class="body">
    <div v-if="!data" class="empty"><span class="spin"></span></div>
    <template v-else>
      <div class="grp" v-if="rows.length">
        <div v-for="w in rows" :key="w.id" class="li" style="cursor:pointer" @click="openDetail(w)">
          <span class="dot" :class="dotFor(w)"></span>
          <div class="t">
            <div class="a">{{ w.name }}</div>
            <div class="b" :class="{ error: w.failed }">{{ subFor(w) }}</div>
          </div>
          <div class="r">
            <button v-if="w.state === 'needs_setup' && !w.in_progress" class="btn sm" :disabled="busy" @click.stop="openPlan(w)">Get ready</button>
            <button v-else-if="w.in_progress" class="btn sm ghost" style="color:var(--accent-hi)" @click.stop="$emit('activity')">Activity</button>
            <span v-else style="color:var(--muted)">›</span>
          </div>
        </div>
      </div>
      <div v-else class="empty">None</div>
    </template>
  </div>
  <div class="foot">
    <span v-if="data" class="n">{{ readyCount }} of {{ data.tools.length }} ready</span>
    <span class="sp"></span>
    <button :disabled="scanning" @click="rescan">{{ scanning ? 'Refreshing…' : 'Refresh' }}</button>
  </div>
  </template>

  <div v-if="sheet" class="sheet-wrap" @click.self="closeSheet">
    <div class="sheet">
      <template v-if="sheet.loading">
        <h5>{{ sheet.w.name }}</h5>
        <p><span class="spin"></span></p>
      </template>
      <template v-else-if="sheet.error">
        <h5>Setup unavailable</h5>
        <p>{{ sheet.error }}</p>
        <div class="acts"><button class="btn" @click="closeSheet">Close</button></div>
      </template>
      <template v-else>
        <h5>Set up {{ sheet.plan.name }}</h5>
        <p>Downloads to {{ sheet.plan.target }}<template v-if="sheet.plan.total_size"> · {{ fmtBytes(sheet.plan.total_size) }}<span v-if="sheet.plan.size_unknown"> + {{ sheet.plan.size_unknown }} unsized</span></template><span v-if="sheet.plan.free_space"> · {{ fmtBytes(sheet.plan.free_space) }} free</span></p>
        <div class="lst">
          <div v-for="d in downloadsToDo" :key="d.filename"><span class="mono f" :title="d.filename">{{ d.filename }}</span><span class="mono">{{ d.size ? fmtBytes(d.size) : '' }}</span></div>
        </div>
        <div v-for="(b, i) in sheet.plan.blockers" :key="i" class="warn">
          <template v-if="b.kind === 'offline'">The adapter is running offline, so downloads are disabled. Install this model in Draw Things instead.</template>
        </div>
        <div v-if="lowSpace" class="warn">Free space is close to the download size. Make room before starting.</div>
        <div class="acts">
          <button class="btn" @click="closeSheet">{{ canStart ? 'Cancel' : 'Close' }}</button>
          <button v-if="canStart" class="btn primary" :disabled="starting" @click="start">{{ starting ? 'Starting…' : 'Download' }}</button>
        </div>
      </template>
    </div>
  </div>
</template>

<script setup>
import { computed, onMounted, onUnmounted, ref } from 'vue'
import { api, fmtBytes } from './api'
const props = defineProps({ overview: Object })
const emit = defineEmits(['activity', 'refresh'])

const data = ref(null)
const filter = ref('all')
const scanning = ref(false)
const sheet = ref(null)
const detail = ref(null)
const starting = ref(false)
let timer = null

const busy = computed(() => !!props.overview?.operation)
async function load() { try { data.value = await api.tools() } catch (e) { /* keep last */ } }
onMounted(() => { load(); timer = setInterval(load, 3000) })
onUnmounted(() => clearInterval(timer))

const all = computed(() => data.value?.tools || [])
const readyCount = computed(() => all.value.filter(w => w.state === 'ready').length)
const filters = computed(() => [
  { id: 'all', label: 'All', count: all.value.length },
  { id: 'ready', label: 'Ready', count: readyCount.value },
  { id: 'needs_setup', label: 'Needs download', count: all.value.filter(w => w.state === 'needs_setup').length },
  { id: 'video', label: 'Video', count: all.value.filter(w => w.kind === 'video').length },
])
const rows = computed(() => all.value.filter(w =>
  filter.value === 'all' ? true : filter.value === 'video' ? w.kind === 'video' : w.state === filter.value))
const otherVariants = computed(() => (detail.value?.variants || []).filter(v => !v.selected))
const missingSummary = computed(() => {
  const d = detail.value
  if (!d?.missing) return ''
  const files = `${d.missing} file${d.missing === 1 ? '' : 's'} missing`
  return d.total_size ? `${files} · ${fmtBytes(d.total_size)}` : files
})

function dotFor(w) {
  if (w.in_progress) return 'b'
  if (w.state === 'ready') return 'g'
  return 'z'
}
function subFor(w) {
  const parts = [(w.tasks || []).join(' · ')]
  if (w.in_progress) parts.push('Downloading')
  else if (w.summary) parts.push(w.summary)
  return parts.filter(Boolean).join(' · ')
}
async function rescan() { scanning.value = true; try { await api.action('refresh'); await load(); emit('refresh') } catch (e) { alert(e.message) } finally { scanning.value = false } }

async function openDetail(w, file) {
  detail.value = { id: w.id, name: w.name, state: w.state, loading: true }
  try { detail.value = await api.toolDetail(w.id, file) }
  catch (e) { detail.value = { id: w.id, name: w.name, state: w.state, error: e.message } }
}
function openPlanFromDetail() {
  const d = detail.value
  detail.value = null
  openPlan({ id: d.id, name: d.name }, d.file)
}
async function openPlan(w, file) {
  sheet.value = { w, loading: true }
  try { const plan = await api.toolDetail(w.id, file); sheet.value = { w, plan } }
  catch (e) { sheet.value = { w, error: e.message } }
}
function closeSheet() { sheet.value = null }
const downloadsToDo = computed(() => (sheet.value?.plan?.files || []).filter(d => !d.installed))
const lowSpace = computed(() => {
  const p = sheet.value?.plan
  return p && p.total_size && p.free_space && p.free_space < p.total_size + 2 * 2 ** 30
})
const canStart = computed(() => {
  const p = sheet.value?.plan
  if (!p || !downloadsToDo.value.length) return false
  return !(p.blockers || []).length
})
async function start() {
  const { w, plan } = sheet.value
  starting.value = true
  try {
    await api.setup(w.id, plan.file)
    closeSheet()
    await load()
    emit('refresh')
    emit('activity')
  } catch (e) { alert(e.message) } finally { starting.value = false }
}
</script>
