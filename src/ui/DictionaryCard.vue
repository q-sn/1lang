<script setup lang="ts">
import { computed } from 'vue';
import { Mic } from '@lucide/vue';
import { useI18n } from 'vue-i18n';
import type { DictionaryEntry } from '@/lib/api';
import { useSpeech } from '@/lib/speech';
import SpeakButton from './SpeakButton.vue';

const props = defineProps<{ entry: DictionaryEntry; sourceLang: string | null; owner?: string }>();
const { t } = useI18n();
const speech = useSpeech();
const owner = computed(() => props.owner ?? 'dict');
/** The word was last played from a native speaker recording. */
const recording = computed(() => {
  const s = speech.sources[owner.value];
  return s?.type === 'recording' ? s : null;
});
</script>

<template>
  <div class="selectable space-y-3 text-[var(--text-size)]">
    <div class="flex flex-wrap items-baseline gap-x-2.5 gap-y-1">
      <span class="font-display text-[1.35em] font-semibold leading-tight">{{ entry.word }}</span>
      <span v-if="entry.lemma && entry.lemma !== entry.word" class="text-muted">→ {{ entry.lemma }}</span>
      <span v-if="entry.transcription" class="font-mono text-[0.85em] text-muted">/{{ entry.transcription }}/</span>
      <SpeakButton :text="entry.word" :lang="sourceLang" :owner="owner" small class="self-center" />
      <span
        v-if="recording"
        class="inline-flex items-center gap-1 self-center text-[12px] text-faint"
        :title="t('speech.recordingTip')"
      >
        <Mic class="size-3.5" />{{ t('speech.recording') }}<template v-if="recording.accent"> · {{ recording.accent }}</template>
      </span>
    </div>

    <ol class="space-y-2.5">
      <li v-for="(s, i) in entry.senses ?? []" :key="i" class="flex gap-2.5">
        <span class="bg-accent mt-[3px] flex size-[22px] shrink-0 items-center justify-center rounded-full text-[12px] font-bold text-white">
          {{ i + 1 }}
        </span>
        <div class="min-w-0 space-y-0.5">
          <div>
            <span v-if="s.pos" class="mr-1.5 text-[0.8em] italic text-muted">{{ s.pos }}</span>
            <span class="font-semibold">{{ (s.translations ?? []).join(', ') }}</span>
          </div>
          <div v-if="s.meaning" class="text-[0.9em] text-muted">{{ s.meaning }}</div>
          <div v-for="(ex, j) in s.examples ?? []" :key="j" class="mt-1 border-l-[3px] border-accent/40 pl-3 text-[0.88em]">
            <div>{{ ex.source }}</div>
            <div class="text-muted">{{ ex.target }}</div>
          </div>
        </div>
      </li>
    </ol>

    <div v-if="entry.forms" class="text-[0.88em]">
      <span class="text-muted">{{ t('dict.forms') }}:</span> {{ entry.forms }}
    </div>
    <div v-if="entry.note" class="rounded-xl bg-nav-selected px-3.5 py-2.5 text-[0.88em] text-muted">{{ entry.note }}</div>
  </div>
</template>
