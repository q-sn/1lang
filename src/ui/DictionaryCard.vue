<script setup lang="ts">
import { useI18n } from 'vue-i18n';
import type { DictionaryEntry } from '@/lib/api';

/** Translations of a looked-up word. The word itself and its pronunciation
 *  belong to the source side (see SourceWord). */
defineProps<{ entry: DictionaryEntry; text: string }>();
const { t } = useI18n();
</script>

<template>
  <div class="selectable space-y-3 text-[var(--text-size)]">
    <div v-if="text" class="flex flex-wrap items-baseline gap-x-2.5 gap-y-1">
      <span class="result-text">{{ text }}</span>
      <!-- Only when the shown text is the model's own pick, not a fallback. -->
      <span
        v-if="entry.translation_transcription && entry.translation === text"
        class="font-mono text-[0.85em] text-muted"
      >/{{ entry.translation_transcription }}/</span>
    </div>
    <ol class="space-y-2.5 border-t border-line pt-3">
      <li v-for="(s, i) in entry.senses ?? []" :key="i" class="flex gap-2.5">
        <span class="bg-accent mt-[3px] flex size-[22px] shrink-0 items-center justify-center rounded-full text-[12px] font-bold text-white">
          {{ i + 1 }}
        </span>
        <div class="min-w-0 space-y-0.5">
          <div>
            <span v-if="s.pos" class="mr-1.5 text-[0.8em] italic text-muted">{{ s.pos }}</span>
            <template v-for="(tr, k) in s.translations ?? []" :key="k">
              <template v-if="k">, </template>
              <span class="font-semibold">{{ tr }}</span>
              <span v-if="s.transcriptions?.[k]" class="ml-1 font-mono text-[0.8em] text-muted">/{{ s.transcriptions[k] }}/</span>
            </template>
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
