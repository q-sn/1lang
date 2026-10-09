<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { useDebounceFn } from '@vueuse/core';
import { ArrowLeftRight, Check, Copy, ScanText, Volume2, X } from '@lucide/vue';
import { useI18n } from 'vue-i18n';
import { commands, events, type TranslationResult } from '@/lib/api';
import { useSettings } from '@/lib/settings';
import { formatCost, speak, useTranslator } from '@/lib/translator';
import Button from '@/ui/Button.vue';
import DictionaryCard from '@/ui/DictionaryCard.vue';
import LanguagePicker from '@/ui/LanguagePicker.vue';
import Select from '@/ui/Select.vue';
import Spinner from '@/ui/Spinner.vue';
import Tip from '@/ui/Tip.vue';

const { t } = useI18n();
const { settings } = useSettings();
const { state, run, reset } = useTranslator();

const source = ref('');
const sourceLang = ref<string | null>(null);
const targetLang = ref<string | null>(null);
const CHAIN = '__chain';
const providerId = ref<string>(CHAIN);
const copied = ref(false);
const textarea = ref<HTMLTextAreaElement | null>(null);

const providerOptions = computed(() => [
  { value: CHAIN, label: t('translate.providerChain') },
  ...settings.value.providers.filter((p) => p.enabled).map((p) => ({ value: p.id, label: p.name })),
]);
const formalityOptions = computed(() =>
  (['default', 'formal', 'informal'] as const).map((v) => ({ value: v, label: t(`translate.formality.${v}`) })),
);

let historyTimer: number | undefined;

async function translateNow() {
  window.clearTimeout(historyTimer);
  const text = source.value;
  if (!text.trim()) {
    reset();
    return;
  }
  const res = await run({
    text,
    sourceLang: sourceLang.value,
    targetLang: targetLang.value,
    providerId: providerId.value === CHAIN ? null : providerId.value,
    origin: 'main',
    saveHistory: false,
  });
  if (res) scheduleHistory(text, res);
}

// The main window translates as you type; history gets the text once it settles.
function scheduleHistory(text: string, res: TranslationResult) {
  window.clearTimeout(historyTimer);
  historyTimer = window.setTimeout(() => {
    if (source.value === text) commands.historyCommit(text, res);
  }, 2500);
}

const debounced = useDebounceFn(translateNow, 450);
watch(source, () => {
  window.clearTimeout(historyTimer);
  if (!source.value.trim()) reset();
  else debounced();
});
watch([sourceLang, targetLang, providerId, () => settings.value.translation.formality], () => {
  if (source.value.trim()) translateNow();
});

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && e.ctrlKey) {
    e.preventDefault();
    translateNow();
  }
}

function swap() {
  // Swap the actual languages (auto-detected ones included), not just the pickers.
  const from = sourceLang.value ?? state.sourceLang;
  const to = targetLang.value ?? state.targetLang;
  if (state.kind === 'text' && state.text && state.status === 'done') source.value = state.text;
  sourceLang.value = to ?? null;
  targetLang.value = from ?? null;
}

async function copy() {
  await commands.copyText(state.text);
  copied.value = true;
  setTimeout(() => (copied.value = false), 1200);
}

function clear() {
  source.value = '';
  textarea.value?.focus();
}

const meta = computed(() => {
  const r = state.result;
  if (!r) return state.providerName ? [state.providerName] : [];
  const parts = [r.model ? `${r.provider_name} · ${r.model}` : r.provider_name];
  if (r.cached) parts.push(t('translate.cached'));
  else {
    parts.push(t('common.ms', { n: r.elapsed_ms }));
    if (r.usage.input_tokens || r.usage.output_tokens)
      parts.push(t('translate.tokens', { input: r.usage.input_tokens, output: r.usage.output_tokens }));
    else if (r.usage.characters) parts.push(t('translate.characters', { n: r.usage.characters }));
    const cost = formatCost(r.cost_usd);
    if (cost && r.cost_usd) parts.push(cost);
  }
  return parts;
});

const busy = computed(() => state.status === 'loading' || state.status === 'streaming');

onMounted(() => {
  textarea.value?.focus();
  events.mainOpenText.listen((e) => {
    source.value = e.payload.text;
    sourceLang.value = null;
    targetLang.value = null;
    if (e.payload.translate) translateNow();
  });
});
</script>

<template>
  <div class="panel-strong relative grid h-full min-h-0 grid-cols-2 overflow-hidden">
    <!-- source -->
    <section class="flex min-h-0 flex-col border-r border-line">
      <div class="flex h-14 shrink-0 items-center gap-2 border-b border-line pl-3 pr-10">
        <LanguagePicker v-model="sourceLang" allow-auto :detected="sourceLang ? null : state.sourceLang" />
      </div>
      <textarea
        ref="textarea"
        v-model="source"
        :placeholder="t('translate.placeholder')"
        spellcheck="false"
        class="result-text min-h-0 flex-1 resize-none bg-transparent px-6 py-5 outline-none placeholder:text-faint"
        @keydown="onKeydown"
      />
      <div class="flex h-14 shrink-0 items-center gap-1 px-3">
        <Tip :text="t('translate.ocr')">
          <Button variant="ghost" size="icon" @click="commands.startOcr()"><ScanText class="size-5" /></Button>
        </Tip>
        <Tip :text="t('translate.speak')">
          <Button variant="ghost" size="icon" :disabled="!source" @click="speak(source, state.sourceLang)">
            <Volume2 class="size-5" />
          </Button>
        </Tip>
        <span v-if="source" class="ml-2 text-[13px] text-faint">{{ t('translate.chars', { n: source.length }) }}</span>
        <div class="flex-1" />
        <Tip v-if="source" :text="t('common.clear')">
          <Button variant="ghost" size="icon" @click="clear"><X class="size-5" /></Button>
        </Tip>
      </div>
    </section>

    <!-- translation -->
    <section class="flex min-h-0 flex-col bg-surface">
      <div class="flex h-14 shrink-0 items-center gap-2 border-b border-line pl-10 pr-3">
        <LanguagePicker
          v-model="targetLang"
          allow-auto
          :auto-label="t('lang.autoTarget')"
          :detected="targetLang ? null : state.targetLang"
          detected-key="lang.autoTo"
        />
        <div class="flex-1" />
        <Select v-model="settings.translation.formality" :options="formalityOptions" size="sm" />
        <Select v-model="providerId" :options="providerOptions" size="sm" trigger-class="max-w-48" />
      </div>
      <div class="min-h-0 flex-1 overflow-y-auto px-6 py-5">
        <div v-if="state.status === 'error'" class="selectable rounded-2xl bg-danger/10 p-4 text-[14px] text-danger">
          {{ state.error }}
          <div class="mt-3">
            <Button size="sm" @click="translateNow">{{ t('common.retry') }}</Button>
          </div>
        </div>
        <DictionaryCard
          v-else-if="state.kind === 'dictionary' && state.dictionary"
          :entry="state.dictionary"
          :source-lang="state.sourceLang"
        />
        <div
          v-else-if="state.text"
          :class="['result-text selectable', state.status === 'streaming' && 'stream-caret']"
        >{{ state.text }}</div>
        <div v-else-if="busy" class="flex items-center gap-2 text-accent">
          <Spinner class="size-5" />
        </div>
        <div v-else class="result-text text-faint">{{ t('translate.empty') }}</div>
      </div>
      <div class="flex h-14 shrink-0 items-center gap-1 px-3">
        <span class="ml-3 truncate text-[12.5px] text-faint">{{ meta.join(' · ') }}</span>
        <div class="flex-1" />
        <Tip :text="t('translate.speak')">
          <Button variant="ghost" size="icon" :disabled="!state.text" @click="speak(state.text, state.targetLang)">
            <Volume2 class="size-5" />
          </Button>
        </Tip>
        <Tip :text="copied ? t('common.copied') : t('common.copy')">
          <Button variant="soft" size="icon" :disabled="!state.text" @click="copy">
            <Check v-if="copied" class="size-5 text-success" />
            <Copy v-else class="size-5" />
          </Button>
        </Tip>
      </div>
    </section>

    <!-- swap -->
    <Tip :text="t('lang.swap')">
      <button
        class="absolute left-1/2 top-7 flex size-11 -translate-x-1/2 -translate-y-1/2 items-center justify-center rounded-full
          border border-line bg-[var(--bg-solid)] text-fg transition-[transform,color] duration-200 hover:rotate-180 hover:text-accent active:scale-90"
        @click="swap"
      >
        <ArrowLeftRight class="size-5" />
      </button>
    </Tip>
  </div>
</template>
