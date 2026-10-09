<script setup lang="ts">
import { TooltipContent, TooltipPortal, TooltipRoot, TooltipTrigger } from 'reka-ui';

defineProps<{ text: string; side?: 'top' | 'bottom' | 'left' | 'right' }>();
</script>

<template>
  <TooltipRoot :delay-duration="400">
    <TooltipTrigger as-child>
      <slot />
    </TooltipTrigger>
    <TooltipPortal>
      <TooltipContent
        :collision-padding="12"
        :side="side ?? 'bottom'"
        :side-offset="8"
        class="z-50 rounded-lg bg-[#1d1a33] px-2.5 py-1.5 text-[12.5px] font-medium text-white animate-fade-in dark:bg-[#ece9ff] dark:text-[#16142b]"
      >
        {{ text }}
      </TooltipContent>
    </TooltipPortal>
  </TooltipRoot>
</template>
