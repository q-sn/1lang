<script setup lang="ts">
defineProps<{ label: string; description?: string; stacked?: boolean }>();
</script>

<template>
  <div
    :class="[
      'flex gap-6 px-5 py-4 [&+&]:border-t [&+&]:border-line',
      stacked ? 'flex-col items-stretch gap-3' : 'items-center justify-between',
    ]"
  >
    <div class="min-w-0">
      <div class="text-[14.5px] font-medium text-fg">{{ label }}</div>
      <div v-if="description" class="mt-1 text-[13px] leading-snug text-muted">{{ description }}</div>
    </div>
    <div :class="stacked ? '' : 'flex shrink-0 items-center gap-2'">
      <slot />
    </div>
  </div>
</template>
