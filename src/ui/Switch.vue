<script setup lang="ts">
import { SwitchRoot, SwitchThumb } from 'reka-ui';

const model = defineModel<boolean>({ required: true });
defineProps<{ disabled?: boolean }>();
</script>

<template>
  <SwitchRoot
    v-model="model"
    :disabled="disabled"
    class="relative inline-flex h-7 w-12 shrink-0 items-center rounded-full bg-line-strong transition-[background] duration-200
      data-[state=checked]:bg-accent disabled:opacity-40"
  >
    <SwitchThumb
      class="block size-5 translate-x-1 rounded-full bg-white transition-transform duration-200
        data-[state=checked]:translate-x-6"
    />
  </SwitchRoot>
</template>
