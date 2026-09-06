<script setup>
defineProps({ primary:Boolean, busy:Boolean, disabled:Boolean })
</script>
<template>
  <button type="button" :disabled="busy || disabled" class="inline-flex shrink-0 items-center justify-center gap-2 rounded-md px-3 py-2 text-xs font-medium transition-colors duration-150" :class="primary ? 'bg-accent text-white hover:bg-accent/90' : 'bg-surface-raised text-content hover:bg-surface-hover'">
    <span v-if="busy" class="h-3 w-3 animate-spin rounded-full border-2 border-current border-t-transparent" aria-hidden="true"></span><slot />
  </button>
</template>
