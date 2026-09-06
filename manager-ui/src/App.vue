<template>
  <div class="app" style="position:relative" ref="rootEl">
    <div class="head">
      <img class="mark" :src="icon" width="18" height="18" alt="" aria-hidden="true" />
      <span class="name">Draw Things</span>
      <span class="sp"></span>
      <span v-if="overview?.state !== 'in_progress'" class="state"><span class="dot" :class="stateDot"></span>{{ stateLabel }}</span>
      <span v-else class="state"><span class="spin tab-spin"></span>{{ overview.summary || 'Working' }}</span>
    </div>
    <div class="tabs">
      <button
        v-for="t in tabs"
        :key="t.id"
        :class="{
          on: tab === t.id,
          attention: t.id === 'activity' && activityAttention,
        }"
        @click="tab = t.id"
      >
        {{ t.label }}
        <template v-if="t.id === 'activity'">
          <span v-if="activityAttention" class="dot a" title="Activity needs attention"></span>
          <span v-else-if="activityActive" class="spin tab-spin" title="Activity in progress"></span>
        </template>
      </button>
    </div>
    <div v-if="overview && overview.summary && !['ready', 'in_progress'].includes(overview.state)" class="banner" :class="{ red: overview.state === 'error' }">
      <span>{{ overview.summary }}</span>
      <button v-if="overview.state === 'warning' && overview.tools_ready === 0" class="lnk" @click="tab = 'tools'">Get a model</button>
      <button v-else-if="overview.state === 'error' && !overview.managed" class="lnk" @click="tab = 'settings'">Details</button>
    </div>
    <div v-if="actionError" class="banner red"><span>{{ actionError }}</span><button class="lnk" @click="actionError = ''">Dismiss</button></div>

    <OverviewTab v-if="tab === 'overview'" :overview="overview" @refresh="load" @open="tab = $event" @error="actionError = $event" />
    <ToolsTab v-else-if="tab === 'tools'" :overview="overview" @activity="tab = 'activity'" @refresh="load" />
    <ActivityTab v-else-if="tab === 'activity'" :overview="overview" @refresh="load" />
    <SettingsTab v-else :overview="overview" @refresh="load" @error="actionError = $event" />
  </div>
</template>

<script setup>
import { computed, onMounted, onUnmounted, ref, nextTick, watch } from 'vue'
import { api } from './api'
import OverviewTab from './OverviewTab.vue'
import ToolsTab from './ToolsTab.vue'
import ActivityTab from './ActivityTab.vue'
import SettingsTab from './SettingsTab.vue'
const icon = new URL('../public/drawthings.png', import.meta.url).href

const tabs = [
  { id: 'overview', label: 'Overview' },
  { id: 'tools', label: 'Tools' },
  { id: 'activity', label: 'Activity' },
  { id: 'settings', label: 'Settings' },
]
const initial = (location.hash || '').replace('#', '')
const tab = ref(tabs.some(t => t.id === initial) ? initial : 'overview')
const overview = ref(null)
const actionError = ref('')
let timer = null
let hostRefreshTimer = null

async function load() {
  try { overview.value = await api.overview() } catch (e) { overview.value = overview.value || { state: 'error', summary: 'Manager unavailable' } }
}
const rootEl = ref(null)
let sizeObs = null
let lastSent = 0
// Tell an embedding host (the Stimma popover) how tall we'd like to be so it
// can size the iframe to content instead of a fixed height.
function reportSize() {
  const root = rootEl.value
  if (!root || window.parent === window) return
  const body = root.querySelector('.body')
  let h = 0
  for (const el of root.children) {
    if (el === body) {
      let inner = 12 // .body vertical padding
      for (const c of body.children) inner += c.offsetHeight || 0
      h += inner
    }
    else if (!el.classList.contains('sheet-wrap')) h += el.offsetHeight || 0
  }
  const sheet = root.querySelector('.sheet')
  if (sheet) h = Math.max(h, sheet.offsetHeight + 80)
  h = Math.ceil(h)
  if (Math.abs(h - lastSent) < 2) return
  lastSent = h
  try { window.parent.postMessage({ type: 'stimma-manage-size', height: h }, '*') } catch { /* */ }
}
onMounted(() => {
  load(); timer = setInterval(load, 2500)
  sizeObs = new MutationObserver(() => nextTick(reportSize))
  if (rootEl.value) sizeObs.observe(rootEl.value, { childList: true, subtree: true, characterData: true, attributes: true })
  nextTick(reportSize)
  window.addEventListener('resize', reportSize)
  window.addEventListener('message', onHostMessage)
})
onUnmounted(() => {
  clearInterval(timer)
  if (hostRefreshTimer) clearTimeout(hostRefreshTimer)
  sizeObs?.disconnect()
  window.removeEventListener('resize', reportSize)
  window.removeEventListener('message', onHostMessage)
})
watch(tab, (t) => { history.replaceState(null, '', `#${t}`); nextTick(reportSize) })
window.addEventListener('hashchange', () => { const h = location.hash.replace('#', ''); if (tabs.some(t => t.id === h)) tab.value = h })

function onHostMessage(e) {
  if (e.data?.type !== 'stimma-manage-refresh') return
  if (hostRefreshTimer) return
  hostRefreshTimer = setTimeout(() => { hostRefreshTimer = null; load() }, 100)
}

const userActivity = computed(() => (overview.value?.activity || []).filter(o => o.kind === 'generation' || o.kind === 'download'))
const activityActive = computed(() => userActivity.value.some(o => o.state === 'running'))
const activityAttention = computed(() => userActivity.value.some(o => o.state === 'failed'))
const stateDot = computed(() => ({ ready: 'g', warning: 'a', error: 'r' }[overview.value?.state] || 'z'))
const stateLabel = computed(() => {
  const s = overview.value?.state
  if (!s) return '…'
  if (s === 'ready') return overview.value.engine?.online ? 'Ready' : (overview.value.managed ? 'On demand' : 'Ready')
  if (s === 'warning') return 'Attention'
  return 'Error'
})
</script>
