<script setup lang="ts">
import { ref } from 'vue';
import { X } from '@lucide/vue';
import { useI18n } from 'vue-i18n';
import { accelFromEvent } from '@/lib/hotkeys';
import Kbd from './Kbd.vue';

const model = defineModel<string>({ required: true });
const { t } = useI18n();
const recording = ref(false);

function onKeydown(e: KeyboardEvent) {
  if (!recording.value) return;
  e.preventDefault();
  e.stopPropagation();
  if (e.key === 'Escape' && !e.ctrlKey && !e.altKey && !e.shiftKey) {
    recording.value = false;
    return;
  }
  const accel = accelFromEvent(e);
  if (accel) {
    model.value = accel;
    recording.value = false;
  }
}
</script>

<template>
  <div class="flex items-center gap-1">
    <button
      :class="[
        'flex h-10 min-w-52 items-center justify-center rounded-xl border px-3 text-[13.5px] transition-colors',
        recording ? 'border-accent bg-surface-strong text-fg' : 'border-line bg-surface-strong hover:bg-hover',
      ]"
      @click="recording = !recording"
      @keydown="onKeydown"
      @blur="recording = false"
    >
      <span v-if="recording">{{ t('settings.hotkeys.record') }}</span>
      <Kbd v-else-if="model" :accel="model" />
      <span v-else class="text-faint">{{ t('settings.hotkeys.none') }}</span>
    </button>
    <button
      v-if="model && !recording"
      class="flex size-9 items-center justify-center rounded-xl text-muted hover:bg-hover hover:text-fg"
      :title="t('common.clear')"
      @click="model = ''"
    >
      <X class="size-4" />
    </button>
  </div>
</template>
