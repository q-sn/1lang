<script setup lang="ts">
const model = defineModel<string | number | null>();
defineProps<{ placeholder?: string; type?: string; mono?: boolean; size?: 'sm' | 'md' }>();
</script>

<template>
  <input
    v-model="model"
    :type="type ?? 'text'"
    :placeholder="placeholder"
    spellcheck="false"
    :class="[
      'w-full border border-line bg-surface-strong px-3.5 text-[14px] text-fg outline-none transition-[border,box-shadow]',
      'placeholder:text-faint focus:border-line-strong',
      size === 'sm' ? 'h-8 rounded-[10px]' : 'h-10 rounded-xl',
      mono && 'font-mono text-[13px]',
    ]"
  />
</template>
