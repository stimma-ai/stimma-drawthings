<script setup>
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue'
import Button from './Button.vue'
const tabs = ['Overview','Models','Activity','Connection']
const tab = ref(tabs.find(t => t.toLowerCase() === location.hash.slice(1)) || 'Overview')
const data = ref(null), error = ref(''), pending = ref(false), search = ref(''), copied = ref(false)
const modelView = ref('Downloaded'), expandedModel = ref(null), gpuHistory = ref([])
const apiBase = new URL('./api/', location.href)
const icon = new URL('../public/drawthings.png', import.meta.url).href
const statusClasses = { done:'text-success', failed:'text-failure', running:'text-running', cancelled:'text-content-tertiary' }
const modelGroups = computed(() => {
  const seen = new Set()
  return (data.value?.profiles || []).filter(p => {
    const key = p.files.map(f => f.file).sort().join('|')
    if (seen.has(key)) return false
    seen.add(key)
    return true
  })
})
const downloadedModels = computed(() => {
  const files = new Map()
  for (const group of modelGroups.value) for (const file of group.files) {
    if (file.installed) files.set(file.file, {...file, kind:group.kind})
  }
  return [...files.values()].sort((a,b) => a.name.localeCompare(b.name))
})
const matches = value => value.toLowerCase().includes(search.value.toLowerCase())
const visibleDownloads = computed(() => downloadedModels.value.filter(f => matches(f.name)))
const browseModels = computed(() => modelGroups.value.filter(p => matches(p.name) || p.files.some(f => matches(f.name))))
const downloaded = computed(() => downloadedModels.value.length)
const metrics = computed(() => data.value?.metrics)
const pressureClass = computed(() => ({normal:'text-success',warning:'text-amber-400',critical:'text-failure'}[metrics.value?.memory_pressure] || 'text-content-secondary'))
const memoryPercent = computed(() => Math.min(100,100 * (metrics.value?.memory_used_bytes || 0) / (metrics.value?.memory_total_bytes || 1)))
const gpuLine = computed(() => gpuHistory.value.map((n,i) => `${i*100/29},${30-n*.28}`).join(' '))
function bytes(n) { return n == null ? '—' : `${(n/1073741824).toFixed(1)} GB` }
function modelActivity(file) { return recent.value.find(a => a.file === file) }
function downloadFor(file) { const a = modelActivity(file); return a?.state === 'running' ? a : null }
const busy = computed(() => pending.value || data.value?.busy)
const status = computed(() => !data.value ? 'Connecting' : data.value.busy ? 'Working' : data.value.engine_online ? 'Ready' : data.value.managed ? 'On demand' : 'Disconnected')
const statusColor = computed(() => !data.value ? 'text-content-tertiary' : data.value.busy ? 'text-running' : data.value.engine_online ? 'text-success' : 'text-content-tertiary')
const recent = computed(() => data.value?.activity || [])
let timer, observer, refreshTimer, loading = false
async function request(path, body) {
  const r = await fetch(new URL(path, apiBase), { method:body ? 'POST':'GET', cache:'no-store', headers:body ? {'Content-Type':'application/json'} : {}, body:body ? JSON.stringify(body) : undefined })
  const json = await r.json()
  if (!r.ok) throw new Error(json.error || `Request failed (${r.status})`)
  return json
}
async function load() {
  if (loading) return
  loading = true
  try {
    data.value = await request('overview'); error.value = ''
    const gpu = data.value.metrics?.gpu_utilization
    if (gpu != null) gpuHistory.value = [...gpuHistory.value, gpu].slice(-30)
  } catch(e) { error.value = e.message } finally { loading = false }
  nextTick(reportSize)
}
async function action(name, file) {
  pending.value = true; error.value = ''
  try { await request('action', {action:name, ...(file ? {file}: {})}); await load() } catch(e) { error.value = e.message } finally { pending.value = false }
}
function changeTab(t) { tab.value = t; history.replaceState(null,'',`#${t.toLowerCase()}`); nextTick(reportSize) }
function age(ts) { const minutes = Math.max(0,Math.floor((Date.now()/1000-ts)/60)); return minutes < 1 ? 'Just now' : `${minutes} min ago` }
async function copyConnection() {
  try { await navigator.clipboard.writeText(data.value.stp_url); copied.value = true; setTimeout(() => copied.value=false,2000) }
  catch { const input = document.querySelector('#connection-url'); input?.focus(); input?.select() }
}
function reportSize() {
  if (parent === window) return
  const main = document.querySelector('main')
  const last = main?.lastElementChild
  const content = last ? last.getBoundingClientRect().bottom - main.getBoundingClientRect().top + main.scrollTop + 20 : 200
  const chrome = ['header','nav'].reduce((sum,tag) => sum + document.querySelector(tag).offsetHeight,0)
  parent.postMessage({type:'stimma-manage-size',height:Math.min(720,Math.max(380,Math.ceil(content+chrome)))},'*')
}
function hostMessage(e) { if (e.source === parent && e.data?.type === 'stimma-manage-refresh') { clearTimeout(refreshTimer); refreshTimer=setTimeout(load,150) } }
onMounted(() => { load(); timer=setInterval(load,2500); observer=new ResizeObserver(reportSize); observer.observe(document.querySelector('main')); window.addEventListener('message',hostMessage) })
onUnmounted(() => { clearInterval(timer); clearTimeout(refreshTimer); observer?.disconnect(); window.removeEventListener('message',hostMessage) })
</script>

<template>
  <div class="mx-auto flex h-dvh max-w-2xl flex-col overflow-hidden">
    <header class="flex shrink-0 items-center gap-3 px-6 py-5">
      <img :src="icon" alt="" class="h-8 w-8 shrink-0 rounded-md">
      <div class="min-w-0 flex-1"><h1 class="font-brand text-lg font-semibold leading-tight">Draw Things</h1><p class="mt-0.5 text-xs text-content-tertiary">{{ data?.managed ? 'Local generation' : 'Connected engine' }}</p></div>
      <span class="flex items-center gap-1.5 whitespace-nowrap text-xs" :class="statusColor" role="status"><span class="status-dot bg-current"></span>{{ status }}</span>
    </header>
    <nav class="flex shrink-0 gap-1 border-b border-edge px-4 pb-2" aria-label="Manager sections">
      <button v-for="t in tabs" :key="t" @click="changeTab(t)" :aria-current="tab === t ? 'page' : undefined" class="rounded-md px-3 py-2 text-xs transition-colors" :class="tab === t ? 'bg-selection/15 text-selection' : 'text-content-secondary hover:bg-surface-hover'">{{ t }}</button>
    </nav>
    <main class="min-h-0 flex-1 overflow-y-auto px-6 py-5">
      <div v-if="error" role="alert" class="mb-5 flex items-start gap-3 text-sm text-failure"><p class="min-w-0 flex-1 break-words">{{ error }}</p><Button @click="load">Retry</Button></div>
      <p v-if="!data" class="py-8 text-center text-content-secondary">Connecting to Draw Things…</p>
      <template v-else-if="tab === 'Overview'">
        <section class="space-y-3">
          <h2 class="section-title">Engine</h2>
          <div class="flex items-center justify-between gap-4">
            <div><p class="font-medium">{{ data.managed ? 'Managed by Stimma' : 'Connected engine' }}</p><p class="mt-1 text-xs leading-relaxed text-content-secondary">{{ data.managed ? 'Starts when you generate. Stop it to free memory.' : 'Running in Draw Things or a separate server.' }}</p></div>
            <Button v-if="data.managed && data.engine_online" :disabled="busy" @click="action('stop')">Stop</Button>
            <Button v-else :primary="data.managed" :busy="pending" :disabled="data.busy" @click="action('start')">{{ data.managed ? 'Start' : 'Connect' }}</Button>
          </div>
          <div v-if="recent[0]?.state === 'running'" class="pt-2" role="status"><div class="flex justify-between gap-3 text-xs"><span class="min-w-0 truncate text-content-secondary">{{ recent[0].detail || recent[0].title }}</span><span class="fact text-running">{{ Math.round((recent[0].progress || 0)*100) }}%</span></div><progress class="mt-2 h-1 w-full accent-accent" max="1" :value="recent[0].progress || 0"></progress><Button v-if="data.operation" class="mt-2" :busy="pending" @click="action('cancel')">Cancel</Button></div>
        </section>
        <section v-if="metrics" class="mt-6">
          <div class="flex items-center justify-between gap-3"><h2 class="section-title">{{ metrics.gpu_name || 'This Mac' }}</h2><span class="text-xs text-content-tertiary">System-wide</span></div>
          <div class="mt-4 flex items-end justify-between gap-4">
            <div><p class="text-xs text-content-secondary">GPU utilization</p><p class="mt-1 font-mono text-2xl tabular-nums">{{ metrics.gpu_utilization == null ? '—' : `${metrics.gpu_utilization}%` }}</p></div>
            <svg v-if="gpuHistory.length > 1" viewBox="0 0 100 32" preserveAspectRatio="none" class="h-12 w-36 text-running" role="img" aria-label="Recent GPU utilization"><polyline :points="gpuLine" fill="none" stroke="currentColor" stroke-width="1.5" vector-effect="non-scaling-stroke" /></svg>
          </div>
          <div class="mt-5 flex items-center justify-between gap-3 text-xs"><span class="text-content-secondary">Unified memory</span><span class="fact">{{ bytes(metrics.memory_used_bytes) }} / {{ bytes(metrics.memory_total_bytes) }}</span></div>
          <div class="mt-2 h-1.5 overflow-hidden rounded-full bg-surface-raised" role="meter" aria-label="Unified memory usage" :aria-valuenow="Math.round(memoryPercent)" aria-valuemin="0" aria-valuemax="100"><div class="h-full rounded-full bg-current" :class="pressureClass" :style="{width:`${memoryPercent}%`}"></div></div>
          <dl class="mt-3 divide-y divide-edge text-xs">
            <div class="flex justify-between py-2.5"><dt class="text-content-secondary">Memory pressure</dt><dd class="capitalize" :class="pressureClass">{{ metrics.memory_pressure || 'Unavailable' }}</dd></div>
            <div v-if="metrics.gpu_memory_bytes != null" class="flex justify-between py-2.5"><dt class="text-content-secondary" title="GPU allocations across this Mac, sharing unified memory">GPU allocations</dt><dd class="fact">{{ bytes(metrics.gpu_memory_bytes) }}</dd></div>
            <div v-if="metrics.engine_rss_bytes != null" class="flex justify-between py-2.5"><dt class="text-content-secondary" title="Resident process memory; GPU allocations are reported separately">Engine resident memory</dt><dd class="fact">{{ bytes(metrics.engine_rss_bytes) }}</dd></div>
            <div class="flex justify-between py-2.5"><dt class="text-content-secondary">Compressed / swap used</dt><dd class="fact">{{ bytes(metrics.compressed_bytes) }} / {{ bytes(metrics.swap_used_bytes) }}</dd></div>
          </dl>
        </section>
        <button class="mt-6 flex w-full items-center justify-between rounded-md bg-surface px-3 py-3 text-left hover:bg-surface-hover" @click="changeTab('Models')"><span class="text-sm">Models</span><span class="text-xs text-content-secondary">{{ downloaded }} downloaded <span class="ml-2" aria-hidden="true">→</span></span></button>
        <section v-if="recent.length" class="mt-6 space-y-3"><div class="flex items-center justify-between"><h2 class="section-title">Recent activity</h2><button class="rounded-md px-2 py-1 text-xs text-accent" @click="changeTab('Activity')">View all</button></div><p class="text-xs text-content-secondary">{{ recent[0].title }} <span class="ml-2" :class="statusClasses[recent[0].state]">{{ recent[0].state === 'done' ? 'Complete' : recent[0].detail }}</span></p></section>
      </template>
      <template v-else-if="tab === 'Models'">
        <div class="flex items-center justify-between gap-3"><h2 class="font-brand text-lg font-semibold">Models</h2><button class="rounded-md px-2 py-1 text-xs text-content-secondary hover:text-content" :disabled="busy" @click="action('refresh')">Refresh</button></div>
        <div class="mt-4 flex gap-1 rounded-lg bg-surface p-1" aria-label="Model library">
          <button v-for="view in ['Downloaded','Browse']" :key="view" class="flex-1 rounded-md px-3 py-2 text-xs" :class="modelView === view ? 'bg-surface-raised text-content' : 'text-content-secondary'" :aria-pressed="modelView === view" @click="modelView=view">{{ view }}<span v-if="view === 'Downloaded'" class="ml-1.5 text-content-tertiary">{{ downloaded }}</span></button>
        </div>
        <input v-model="search" class="field mt-4" placeholder="Search models" aria-label="Search models">
        <template v-if="modelView === 'Downloaded'">
          <p class="mt-3 text-xs leading-relaxed text-content-secondary">{{ data.managed ? 'Checkpoints in your Draw Things folder. Supporting files download when needed.' : 'Checkpoints available on the connected engine.' }}</p>
          <div class="mt-3 divide-y divide-edge"><article v-for="f in visibleDownloads" :key="f.file" class="flex items-center gap-3 py-3.5"><span class="text-success" aria-hidden="true">✓</span><div class="min-w-0 flex-1"><h3 class="text-sm font-medium">{{ f.name }}</h3><p class="mt-1 text-xs capitalize text-content-tertiary">{{ f.kind }}<span v-if="f.size_bytes"> · {{ bytes(f.size_bytes) }}</span></p></div></article></div>
          <div v-if="!visibleDownloads.length" class="py-8 text-center"><p class="text-content-secondary">{{ search ? 'No matching models.' : 'No models downloaded yet.' }}</p><Button v-if="!search" class="mt-4" primary @click="modelView='Browse'">Browse models</Button></div>
        </template>
        <template v-else>
          <p class="mt-3 text-xs leading-relaxed text-content-secondary">Choose a model to see its versions. You can also generate in Stimma and let the adapter download what it needs.</p>
          <div class="mt-3 divide-y divide-edge">
            <section v-for="p in browseModels" :key="p.id" class="py-1">
              <button class="flex w-full items-center gap-3 rounded-md py-3 text-left" :aria-expanded="expandedModel === p.id" @click="expandedModel=expandedModel === p.id ? null : p.id"><div class="min-w-0 flex-1"><h3 class="text-sm font-medium">{{ p.name }}</h3><p class="mt-1 text-xs text-content-tertiary">{{ p.video || p.kind === 'video' ? 'Video' : 'Image' }} · {{ p.files.length }} {{ p.files.length === 1 ? 'version' : 'versions' }}</p></div><span v-if="p.files.some(f=>f.installed)" class="text-xs text-success">Downloaded</span><span class="text-content-tertiary" aria-hidden="true">{{ expandedModel === p.id ? '−' : '+' }}</span></button>
              <div v-if="expandedModel === p.id" class="space-y-4 pb-4 pl-3">
                <div v-for="f in p.files" :key="f.file"><div class="flex items-center gap-3"><p class="min-w-0 flex-1 text-xs leading-relaxed text-content-secondary">{{ f.name }}</p><Button v-if="downloadFor(f.file)" :busy="pending" @click="action('cancel')">Cancel</Button><span v-else-if="f.installed" class="text-xs text-success">Downloaded</span><Button v-else primary :disabled="busy || data.offline" @click="action('install',f.file)">Download</Button></div>
                  <p v-if="modelActivity(f.file)?.state === 'failed'" class="mt-2 text-xs text-failure">{{ modelActivity(f.file).detail }}</p>
                  <div v-if="downloadFor(f.file)" class="mt-2" role="status"><p class="text-xs text-running">{{ downloadFor(f.file).detail }}</p><progress class="mt-2 h-1 w-full accent-accent" max="1" :value="downloadFor(f.file).progress || 0"></progress></div>
                </div>
              </div>
            </section>
          </div>
          <p v-if="!browseModels.length" class="py-8 text-center text-content-secondary">No matching models.</p>
        </template>
      </template>
      <template v-else-if="tab === 'Activity'">
        <h2 class="font-brand text-lg font-semibold">Activity</h2><p class="mt-2 text-xs text-content-secondary">Generations and downloads from this adapter session.</p>
        <div v-if="recent.length" class="mt-5 space-y-5"><article v-for="item in recent" :key="item.id"><div class="flex items-start justify-between gap-3"><p class="min-w-0 break-words text-sm font-medium">{{ item.title }}</p><span class="fact shrink-0 text-content-tertiary">{{ age(item.started_at) }}</span></div><p class="mt-1 break-words text-xs" :class="statusClasses[item.state]">{{ item.detail || item.state }}</p><progress v-if="item.state === 'running'" class="mt-2 h-1 w-full accent-accent" max="1" :value="item.progress || 0"></progress></article></div>
        <div v-else class="py-10 text-center"><h3 class="font-brand text-lg font-semibold">Ready when you are</h3><p class="mt-2 text-sm text-content-secondary">Start a generation in Stimma to see its progress here.</p></div>
      </template>
      <template v-else>
        <h2 class="font-brand text-lg font-semibold">Connection</h2><p class="mt-2 text-xs leading-relaxed text-content-secondary">Use this provider in Stimma or with the STP command-line tool.</p>
        <label for="connection-url" class="section-title mt-5 block">Provider URL</label><div class="mt-2 flex items-center gap-2"><input id="connection-url" :value="data.stp_url" readonly class="min-w-0 flex-1 bg-transparent py-2 font-mono text-xs" @click="$event.target.select()"><Button @click="copyConnection">{{ copied ? 'Copied' : 'Copy' }}</Button></div>
        <h3 class="section-title mt-6">From your terminal</h3><pre class="fact mt-3 whitespace-pre-wrap break-all text-content-secondary">stp --url {{ data.stp_url }} tools</pre>
        <dl class="mt-6 divide-y divide-edge text-xs"><div class="flex justify-between gap-3 py-2.5"><dt class="text-content-secondary">Adapter version</dt><dd class="fact">{{ data.version }}</dd></div><div class="flex justify-between gap-3 py-2.5"><dt class="text-content-secondary">Engine mode</dt><dd>{{ data.managed ? 'Managed' : 'Attached' }}</dd></div><div class="flex justify-between gap-3 py-2.5"><dt class="text-content-secondary">Downloads</dt><dd>{{ data.offline ? 'Disabled (offline)' : 'On demand' }}</dd></div></dl>
        <template v-if="data.models_dir"><h3 class="section-title mt-6">Model folder</h3><p class="fact mt-3 break-all text-content-secondary">{{ data.models_dir }}</p></template>
      </template>
    </main>

  </div>
</template>
