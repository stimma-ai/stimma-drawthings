<template>
  <div class="body">
    <template v-if="!overview"><div class="empty"><span class="spin"></span></div></template>
    <template v-else>
      <div v-for="h in overview.hosts" :key="h.host" class="grp">
        <h4>{{ hostLabel(h) }} <span class="n" v-if="!h.reachable">unreachable</span></h4>
        <div v-if="h.gpus && h.gpus.length" class="gpus">
          <div v-for="g in h.gpus" :key="g.index" class="gpu" :class="{ down: !h.reachable }">
            <div class="k" :title="g.name || ''">GPU</div>
            <div class="v mono">{{ g.util != null ? Math.round(g.util) + '%' : '—' }}</div>
            <div class="bar" :class="pressureClass(h)"><i :style="{ width: memPct(g) + '%' }"></i></div>
            <div class="k mono" style="margin-top:3px">{{ mem(g) }}</div>
          </div>
          <div class="gpu">
            <div class="k">Engine</div>
            <div class="v mono" :class="{ dim: !h.engine_online }">{{ h.engine_online ? (h.engine_rss != null ? fmtBytes(h.engine_rss) : 'Running') : (h.engine_managed ? 'Idle' : 'Offline') }}</div>
            <div class="bar" :class="pressureClass(h)"><i :style="{ width: enginePct(h) + '%' }"></i></div>
            <div class="k mono" style="margin-top:3px">{{ pressureLabel(h) }}</div>
          </div>
        </div>

      </div>

      <div class="grp" v-if="overview.running.length">
        <h4>Running</h4>
        <div v-for="j in overview.running" :key="j.id" class="li">
          <div class="t">
            <div class="a">{{ j.title }}</div>
            <div class="bar" :class="{ ind: j.progress == null }"><i :style="{ width: (j.progress != null ? Math.round(j.progress * 100) : 40) + '%' }"></i></div>
            <div class="b" style="margin-top:4px">{{ jobSub(j) }}</div>
          </div>
        </div>
      </div>
      <div class="grp" v-if="overview.pending">
        <h4>Queued <span class="n mono">{{ overview.pending }}</span></h4>
      </div>
    </template>
  </div>
  <div class="foot" v-if="overview">
    <span>Draw Things adapter <span class="mono">{{ overview.version }}</span></span>
    <span class="sp"></span>
    <span v-if="overview.disk && overview.disk.length" class="mono">{{ fmtBytes(overview.disk[0].free) }} free</span>
  </div>
</template>

<script setup>
import { fmtBytes, fmtElapsed } from './api'
const props = defineProps({ overview: Object })
defineEmits(['refresh', 'open', 'error'])

function hostLabel(h) { return h.local ? (h.hostname || 'This Mac') : h.host }
function memPct(g) { return g.mem_total ? Math.min(100, Math.round(100 * (g.mem_used || 0) / g.mem_total)) : 0 }
function mem(g) { return g.mem_total ? `${Math.round((g.mem_used || 0) / 2 ** 30)} / ${Math.round(g.mem_total / 2 ** 30)} GB` : '' }
function enginePct(h) {
  const total = h.gpus?.[0]?.mem_total
  if (!total || h.engine_rss == null || !h.engine_online) return 0
  return Math.min(100, Math.round(100 * h.engine_rss / total))
}
function pressureClass(h) { return { warning: 'warn', critical: 'crit' }[h.memory_pressure] || '' }
function pressureLabel(h) {
  const p = h.memory_pressure
  return p && p !== 'normal' ? `${p} memory pressure` : ''
}
function jobSub(j) {
  const parts = []
  if (j.started_at) parts.push(fmtElapsed(j.started_at) + ' elapsed')
  if (j.detail && j.detail !== 'Starting') parts.push(j.detail)
  return parts.join(' · ')
}
</script>
