<template>
  <div class="body">
    <div v-if="!overview" class="empty"><span class="spin"></span></div>
    <template v-else>
      <div class="grp" v-if="active.length">
        <h4>In progress</h4>
        <div v-for="o in active" :key="o.id" class="li">
          <div class="t">
            <div class="a">{{ o.title }}</div>
            <div v-if="o.kind !== 'generation'" class="bar" :class="{ ind: o.progress == null || o.progress === 0 }"><i :style="{ width: Math.round((o.progress || 0) * 100) + '%' }"></i></div>
            <div v-else class="bar" :class="{ ind: o.progress == null }"><i :style="{ width: (o.progress != null ? Math.round(o.progress * 100) : 40) + '%' }"></i></div>
            <div class="b" style="margin-top:4px">{{ o.detail || 'Running' }}<span v-if="o.started_at"> · {{ fmtElapsed(o.started_at) }}</span></div>
          </div>
          <div class="r">
            <button v-if="o.kind !== 'generation' && overview.operation === o.id" class="btn sm ghost" title="Cancel" @click="cancel">✕</button>
          </div>
        </div>
      </div>
      <div class="grp" v-if="done.length">
        <h4>Done</h4>
        <div v-for="o in done" :key="o.id" class="li">
          <span class="dot" :class="o.state === 'done' ? 'g' : o.state === 'failed' ? 'r' : 'z'"></span>
          <div class="t">
            <div class="a">{{ o.title }}</div>
            <div class="b" :class="{ error: o.state === 'failed' }">{{ o.state === 'failed' ? (o.detail || 'Failed') : (o.state === 'cancelled' ? 'Cancelled' : (o.detail || 'Done')) }}<span v-if="o.finished_at"> · {{ fmtAgo(o.finished_at) }}</span></div>
          </div>
          <div class="r">
            <button v-if="o.state === 'failed' && o.kind === 'download' && o.file" class="btn sm" :disabled="!!overview.operation" @click="retry(o)">Retry</button>
          </div>
        </div>
      </div>
      <div v-if="!active.length && !done.length" class="empty">No activity yet</div>
    </template>
  </div>
  <div class="foot" v-if="done.length">
    <span class="sp"></span>
    <button @click="clearDone">Clear done</button>
  </div>
</template>

<script setup>
import { computed } from 'vue'
import { api, fmtAgo, fmtElapsed } from './api'
const props = defineProps({ overview: Object })
const emit = defineEmits(['refresh'])
// Activity is what the user asked for: generations and model downloads.
// Engine start/stop and catalog refreshes are plumbing and stay out.
const ops = computed(() => (props.overview?.activity || []).filter(o => o.kind === 'generation' || o.kind === 'download'))
const active = computed(() => ops.value.filter(o => o.state === 'running'))
const done = computed(() => ops.value.filter(o => o.state !== 'running'))
async function cancel() { try { await api.action('cancel'); emit('refresh') } catch (e) { alert(e.message) } }
async function retry(o) { try { await api.action('install', o.file); emit('refresh') } catch (e) { alert(e.message) } }
async function clearDone() { try { await api.clearDone(); emit('refresh') } catch (e) { alert(e.message) } }
</script>
