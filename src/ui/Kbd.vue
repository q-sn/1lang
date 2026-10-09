<script setup lang="ts">
import { computed } from 'vue';
import { prettyAccel } from '@/lib/hotkeys';

const props = defineProps<{ accel: string }>();
const keys = computed(() => prettyAccel(props.accel));
</script>

<template>
  <span class="inline-flex items-center gap-1">
    <kbd
      v-for="k in keys"
      :key="k"
      class="inline-flex h-6 min-w-6 items-center justify-center rounded-md border border-line-strong border-b-[3px] bg-surface-strong px-1.5 font-sans text-[12px] font-semibold text-muted"
    >
      {{ k }}
    </kbd>
  </span>
</template>
