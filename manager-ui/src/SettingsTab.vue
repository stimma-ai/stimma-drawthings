<template>
  <div class="body">
    <div v-if="!overview" class="empty"><span class="spin"></span></div>
    <template v-else>
      <div class="grp">
        <h4>Engine</h4>
        <div class="li">
          <div class="t">
            <div class="a">{{ engineTitle }}</div>
          </div>
          <div class="r">
            <button v-if="overview.managed && overview.engine.online" class="btn sm" :disabled="busy || overview.busy" @click="act('stop')">{{ busy === 'stop' ? 'Stopping…' : 'Stop' }}</button>
            <button v-else-if="overview.managed" class="btn sm" :disabled="busy || overview.busy" @click="act('start')">{{ busy === 'start' ? 'Starting…' : 'Start' }}</button>
            <span v-else :class="overview.engine.online ? '' : 'error'">{{ overview.engine.online ? 'Online' : 'Offline' }}</span>
          </div>
        </div>
        <div class="li" v-if="!overview.managed">
          <div class="t"><div class="a">Endpoint</div><div class="b mono">{{ overview.engine.endpoint }}</div></div>
        </div>
        <div class="li">
          <div class="t"><div class="a">Draw Things model catalog</div><div v-if="overview.offline" class="b">Offline · downloads disabled</div></div>
          <div class="r"><button class="btn sm" :disabled="busy || overview.busy" @click="act('refresh')">{{ busy === 'refresh' ? 'Refreshing…' : 'Refresh' }}</button></div>
        </div>
      </div>
      <div class="grp">
        <h4>Draw Things adapter</h4>
        <div class="li">
          <div class="t"><div class="a">Version</div><div class="b mono">{{ overview.version }}</div></div>
        </div>
      </div>
    </template>
  </div>
</template>

<script setup>
import { computed, ref } from 'vue'
import { api } from './api'
const props = defineProps({ overview: Object })
const emit = defineEmits(['refresh', 'error'])
const busy = ref('')
const engineTitle = computed(() => {
  const o = props.overview
  if (!o) return ''
  if (o.engine.online) return 'Draw Things is running'
  return o.managed ? 'Draw Things is idle' : 'Draw Things is offline'
})
async function act(action) {
  busy.value = action
  try { await api.action(action); emit('refresh') }
  catch (e) { emit('error', e.message) }
  finally { busy.value = '' }
}
</script>
