<script setup lang="ts" generic="T extends string | number">
import {
  SelectContent,
  SelectIcon,
  SelectItem,
  SelectItemIndicator,
  SelectItemText,
  SelectPortal,
  SelectRoot,
  SelectTrigger,
  SelectViewport,
} from 'reka-ui';
import { Check, ChevronDown } from '@lucide/vue';

const model = defineModel<T>({ required: true });
defineProps<{
  options: { value: T; label: string }[];
  size?: 'sm' | 'md';
  disabled?: boolean;
  triggerClass?: string;
}>();
</script>

<template>
  <SelectRoot v-model="model as any" :disabled="disabled">
    <SelectTrigger
      :class="[
        'inline-flex min-w-0 items-center justify-between gap-2 border border-line bg-surface-strong text-left',
        'transition-colors hover:bg-hover data-[state=open]:bg-hover disabled:opacity-40',
        size === 'sm' ? 'h-8 rounded-[10px] px-3 text-[13px]' : 'h-10 rounded-xl px-3.5 text-[14px]',
        triggerClass,
      ]"
    >
      <span class="truncate">{{ options.find((o) => o.value === model)?.label }}</span>
      <SelectIcon><ChevronDown class="size-4 text-faint" /></SelectIcon>
    </SelectTrigger>
    <SelectPortal>
      <SelectContent
        :collision-padding="12"
        position="popper"
        :side-offset="6"
        class="menu z-50 max-h-[min(380px,var(--reka-select-content-available-height))] min-w-[var(--reka-select-trigger-width)] overflow-hidden animate-pop-in"
      >
        <SelectViewport class="p-1.5">
          <SelectItem
            v-for="o in options"
            :key="String(o.value)"
            :value="o.value"
            class="relative flex h-10 cursor-default select-none items-center rounded-[10px] pl-9 pr-4 text-[14px] outline-none
              data-[highlighted]:bg-hover data-[state=checked]:font-semibold"
          >
            <SelectItemIndicator class="absolute left-3 inline-flex">
              <Check class="size-4 text-accent" />
            </SelectItemIndicator>
            <SelectItemText>{{ o.label }}</SelectItemText>
          </SelectItem>
        </SelectViewport>
      </SelectContent>
    </SelectPortal>
  </SelectRoot>
</template>
