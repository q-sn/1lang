<script setup lang="ts">
import { computed } from 'vue';

const props = withDefaults(
  defineProps<{
    variant?: 'primary' | 'secondary' | 'ghost' | 'danger' | 'soft';
    size?: 'sm' | 'md' | 'lg' | 'icon' | 'icon-sm' | 'icon-lg';
    active?: boolean;
    disabled?: boolean;
    type?: 'button' | 'submit';
  }>(),
  { variant: 'secondary', size: 'md', type: 'button' },
);

const classes = computed(() => [
  'inline-flex items-center justify-center gap-2 shrink-0 select-none whitespace-nowrap font-medium',
  'transition-[background-color,color,box-shadow,transform,opacity] duration-150 active:scale-[0.97]',
  'disabled:opacity-40 disabled:pointer-events-none',
  {
    sm: 'h-8 rounded-[10px] px-3 text-[13px]',
    md: 'h-10 rounded-xl px-4 text-[14px]',
    lg: 'h-12 rounded-2xl px-5 text-[15px]',
    icon: 'size-10 rounded-xl',
    'icon-sm': 'size-8 rounded-[10px]',
    'icon-lg': 'size-12 rounded-2xl',
  }[props.size],
  {
    primary:
      'bg-accent text-on-accent hover:bg-accent-strong',
    secondary: 'bg-surface-strong border border-line hover:bg-hover',
    soft: 'bg-nav-selected text-fg hover:bg-active',
    ghost: 'text-muted hover:text-fg hover:bg-hover',
    danger: 'text-danger hover:bg-danger/10',
  }[props.variant],
  props.active && 'bg-accent! text-on-accent!',
]);
</script>

<template>
  <button :type="type" :class="classes" :disabled="disabled">
    <slot />
  </button>
</template>
