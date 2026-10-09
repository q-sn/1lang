<script setup lang="ts" generic="T extends string">
import { ToggleGroupItem, ToggleGroupRoot } from 'reka-ui';

const model = defineModel<T>({ required: true });
defineProps<{ options: { value: T; label: string }[]; size?: 'sm' | 'md' }>();

function update(v: unknown) {
  // Single toggle groups allow deselecting; keep one option always selected.
  if (typeof v === 'string' && v) model.value = v as T;
}
</script>

<template>
  <ToggleGroupRoot
    type="single"
    :model-value="model"
    class="inline-flex gap-1 rounded-xl border border-line bg-surface p-1"
    @update:model-value="update"
  >
    <ToggleGroupItem
      v-for="o in options"
      :key="o.value"
      :value="o.value"
      :class="[
        'rounded-[9px] font-medium text-muted transition-colors duration-150 hover:text-fg',
        'data-[state=on]:bg-accent data-[state=on]:text-on-accent',
        size === 'sm' ? 'h-7 px-3 text-[13px]' : 'h-8 px-3.5 text-[13.5px]',
      ]"
    >
      {{ o.label }}
    </ToggleGroupItem>
  </ToggleGroupRoot>
</template>
