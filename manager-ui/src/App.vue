<script setup>
import { ref, computed, onMounted, onUnmounted, nextTick } from 'vue'
import Button from './Button.vue'
const tabs = ['Overview','Models','Activity','Connection']
const tab = ref(tabs.find(t => t.toLowerCase() === location.hash.slice(1)) || 'Overview')
const data = ref(null), error = ref(''), pending = ref(false), search = ref(''), installedOnly = ref(false), copied = ref(false)
const selected = ref({})
const apiBase = new URL('./api/', location.href)
const icon = new URL('../public/drawthings.svg', import.meta.url).href
const statusClasses = { done:'text-success', failed:'text-failure', running:'text-running', cancelled:'text-content-tertiary' }
const profiles = computed(() => (data.value?.profiles || []).filter(p => (!installedOnly.value || p.files.some(f => f.installed)) && `${p.name} ${p.id}`.toLowerCase().includes(search.value.toLowerCase())))
const downloaded = computed(() => new Set((data.value?.profiles || []).flatMap(p => p.files.filter(f => f.installed).map(f => f.file))).size)
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
  try { data.value = await request('overview'); error.value = '' } catch(e) { error.value = e.message } finally { loading = false }
  nextTick(reportSize)
}
async function action(name, file) {
  pending.value = true; error.value = ''
  try { await request('action', {action:name, ...(file ? {file}: {})}); await load() } catch(e) { error.value = e.message } finally { pending.value = false }
}
function pick(profile) { return selected.value[profile.id] || profile.files[0]?.file }
function isDownloaded(profile) { return profile.files.find(f => f.file === pick(profile))?.installed }
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
  const chrome = ['header','nav','footer'].reduce((sum,tag) => sum + document.querySelector(tag).offsetHeight,0)
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
      <div class="min-w-0 flex-1"><h1 class="font-brand text-lg font-semibold leading-tight">Draw Things</h1><p class="mt-0.5 text-xs text-content-tertiary">Local generation for Stimma</p></div>
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
        <section class="mt-6 space-y-3">
          <h2 class="section-title">Tools and models</h2>
          <dl class="divide-y divide-edge text-xs">
            <div class="flex justify-between py-2.5"><dt class="text-content-secondary">Available tools</dt><dd class="fact">{{ data.tools_count }}</dd></div>
            <div class="flex justify-between py-2.5"><dt class="text-content-secondary">Downloaded checkpoints</dt><dd class="fact">{{ downloaded }}</dd></div>
            <div class="flex justify-between py-2.5"><dt class="text-content-secondary">LoRAs in catalog</dt><dd class="fact">{{ data.loras_count }}</dd></div>
          </dl>
          <Button @click="changeTab('Models')">Browse models</Button>
        </section>
        <section class="mt-6 space-y-3">
          <h2 class="section-title">Model storage</h2>
          <p class="text-xs leading-relaxed text-content-secondary">{{ data.managed ? 'Uses your Draw Things model folder. Missing files download when a tool needs them.' : 'Models are stored on the connected Draw Things server.' }}</p>
          <p v-if="data.models_dir" class="fact break-all leading-relaxed text-content-secondary">{{ data.models_dir }}</p>
        </section>
        <section v-if="recent.length" class="mt-6 space-y-3"><div class="flex items-center justify-between"><h2 class="section-title">Recent activity</h2><button class="rounded-md px-2 py-1 text-xs text-accent" @click="changeTab('Activity')">View all</button></div><p class="text-xs text-content-secondary">{{ recent[0].title }} <span class="ml-2" :class="statusClasses[recent[0].state]">{{ recent[0].state === 'done' ? 'Complete' : recent[0].detail }}</span></p></section>
      </template>
      <template v-else-if="tab === 'Models'">
        <div class="flex items-center justify-between gap-3"><h2 class="font-brand text-lg font-semibold">Models</h2><Button :busy="pending" :disabled="data.busy" @click="action('refresh')">Refresh</Button></div>
        <p class="mt-2 text-xs leading-relaxed text-content-secondary">Download ahead of time, or let your first generation take care of it. Supporting files are included.</p>
        <input v-model="search" class="field mt-4" placeholder="Search models" aria-label="Search models">
        <label class="mt-3 flex items-center gap-2 text-xs text-content-secondary"><input v-model="installedOnly" type="checkbox" class="accent-accent">Downloaded only</label>
        <div class="mt-5 space-y-5">
          <section v-for="p in profiles" :key="p.id" class="space-y-2"><div class="flex items-center gap-2"><h3 class="flex-1 text-sm font-medium">{{ p.name }}</h3><span v-if="isDownloaded(p)" class="flex items-center gap-1.5 text-xs text-success"><span class="status-dot bg-current"></span>Downloaded</span></div>
            <div class="flex items-center gap-2"><select :value="pick(p)" @change="selected[p.id]=$event.target.value" class="field min-w-0 flex-1 !text-xs" :aria-label="`${p.name} checkpoint`"><option v-for="f in p.files" :key="f.file" :value="f.file">{{ f.name || f.file }}</option></select><Button :disabled="busy || (data.offline && !isDownloaded(p))" @click="action('install',pick(p))">{{ isDownloaded(p) ? 'Check files' : 'Download' }}</Button></div>
            <p class="fact break-all text-content-tertiary">{{ pick(p) }}</p>
          </section>
          <p v-if="!profiles.length" class="py-6 text-center text-content-secondary">No matching models.</p>
        </div>
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
        <p class="mt-6 text-xs leading-relaxed text-content-secondary">The adapter shares tools with every connected STP client. Keep it running while you use Stimma.</p>
      </template>
    </main>
    <footer class="flex shrink-0 items-center justify-between px-6 py-3 text-xs text-content-tertiary"><a href="https://stimma.ai" target="_blank" rel="noreferrer" class="rounded-md hover:text-content">Part of Stimma</a><a href="https://github.com/stimma-ai/stimma-drawthings" target="_blank" rel="noreferrer" class="rounded-md hover:text-content">Docs &amp; source</a></footer>
  </div>
</template>
