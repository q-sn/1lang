<script setup lang="ts">
import { computed } from 'vue';
import { Mic } from '@lucide/vue';
import { useI18n } from 'vue-i18n';
import type { DictionaryEntry } from '@/lib/api';
import { useSpeech } from '@/lib/speech';
import SpeakButton from './SpeakButton.vue';

/** Lemma and pronunciation of a looked-up word, shown on the source side. */
const props = defineProps<{
  entry: DictionaryEntry;
  lang: string | null;
  /** Speech button whose last clip may be a recording. */
  owner: string;
  /** Add own play buttons (when the source side has none). */
  speakable?: boolean;
}>();
const { t } = useI18n();
const speech = useSpeech();
const recording = computed(() => {
  const s = speech.sources[props.owner];
  return s?.type === 'recording' ? s : null;
});
</script>

<template>
  <div class="flex min-w-0 flex-wrap items-center gap-x-2.5 gap-y-1 text-[14px] text-muted">
    <span v-if="entry.lemma && entry.lemma !== entry.word">→ {{ entry.lemma }}</span>
    <SpeakButton v-if="speakable" :text="entry.word" :lang="lang" :owner="owner" small />
    <span v-if="recording" class="inline-flex items-center gap-1 text-[12px] text-faint" :title="t('speech.recordingTip')">
      <Mic class="size-3.5" />{{ t('speech.recording') }}<template v-if="recording.accent"> · {{ recording.accent }}</template>
    </span>
  </div>
</template>
