<script setup lang="ts">
import { computed, onMounted, ref, watch } from 'vue';
import { Check, Copy, Eye, EyeOff, Sparkles, SpellCheck, X } from '@lucide/vue';
import { useI18n } from 'vue-i18n';
import { commands, type CheckLevel } from '@/lib/api';
import { useChecker, useLlmProviders } from '@/lib/checker';
import { useLanguages } from '@/lib/langs';
import { checkText, navigate } from '@/lib/nav';
import { useSettings } from '@/lib/settings';
import { formatCost } from '@/lib/translator';
import Button from '@/ui/Button.vue';
import CheckResultView from '@/ui/CheckResultView.vue';
import Select from '@/ui/Select.vue';
import Tip from '@/ui/Tip.vue';

const { t } = useI18n();
const { name } = useLanguages();
const { settings } = useSettings();
const { state, run, reset, finalText, hasEdits } = useChecker();
const llms = useLlmProviders();

const source = ref('');
const level = ref<CheckLevel>(settings.value.check.default_level);
const CHAIN = '__chain';
const providerId = ref<string>(CHAIN);
const showEdits = ref(true);
const copied = ref(false);
const textarea = ref<HTMLTextAreaElement | null>(null);
/** The text the shown result belongs to. */
const checkedText = ref('');

const levelOptions = computed(() =>
  (['errors', 'natural', 'rewrite'] as const).map((v) => ({ value: v, label: t(`check.level.${v}`) })),
);
const providerOptions = computed(() => [
  { value: CHAIN, label: t('translate.providerChain') },
  ...llms.value.map((p) => ({ value: p.id, label: p.name })),
]);
watch(llms, (list) => {
  if (providerId.value !== CHAIN && !list.some((p) => p.id === providerId.value)) providerId.value = CHAIN;
});

const busy = computed(() => state.status === 'loading');
const stale = computed(() => state.status === 'done' && source.value.trim() !== checkedText.value.trim());

async function check() {
  const text = source.value;
  if (!text.trim() || !llms.value.length) return;
  checkedText.value = text;
  const res = await run({
    text,
    level: level.value,
    providerId: providerId.value === CHAIN ? null : providerId.value,
    origin: 'main',
  });
  // A full rewrite is easier to read as clean text.
  if (res) showEdits.value = res.level !== 'rewrite';
}

watch([level, providerId], () => {
  if (state.status === 'done' || state.status === 'error') check();
});

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Enter' && e.ctrlKey) {
    e.preventDefault();
    check();
  }
}

function apply() {
  source.value = finalText.value;
  reset();
  textarea.value?.focus();
}

async function copy() {
  await commands.copyText(finalText.value);
  copied.value = true;
  setTimeout(() => (copied.value = false), 1200);
}

function clear() {
  source.value = '';
  reset();
  textarea.value?.focus();
}

const meta = computed(() => {
  const r = state.result;
  if (!r) return [];
  const parts = [r.model ? `${r.provider_name} · ${r.model}` : r.provider_name];
  if (r.cached) parts.push(t('translate.cached'));
  else {
    parts.push(t('common.ms', { n: r.elapsed_ms }));
    parts.push(t('translate.tokens', { input: r.usage.input_tokens, output: r.usage.output_tokens }));
    const cost = formatCost(r.cost_usd);
    if (cost && r.cost_usd) parts.push(cost);
  }
  return parts;
});

/** Provider errors that are fixed in the provider settings. */
const setupError = computed(() => /API key|AI model|not configured/i.test(state.error ?? ''));

function takeHandedText() {
  if (checkText.value === null) return;
  source.value = checkText.value;
  checkText.value = null;
  reset();
  check();
}
onMounted(() => {
  textarea.value?.focus();
  takeHandedText();
});
watch(checkText, takeHandedText);
</script>

<template>
  <div class="panel-strong relative grid h-full min-h-0 grid-cols-2 overflow-hidden">
    <!-- the user's text -->
    <section class="flex min-h-0 flex-col border-r border-line">
      <div class="flex h-14 shrink-0 items-center gap-2 border-b border-line px-5">
        <SpellCheck class="size-[18px] text-muted" />
        <span class="truncate text-[14px] font-medium text-muted">
          {{ state.result?.lang ? name(state.result.lang) : t('check.yourText') }}
        </span>
      </div>
      <textarea
        ref="textarea"
        v-model="source"
        :placeholder="t('check.placeholder')"
        spellcheck="false"
        class="result-text min-h-0 flex-1 resize-none bg-transparent px-6 py-5 outline-none placeholder:text-faint"
        @keydown="onKeydown"
      />
      <div class="flex h-14 shrink-0 items-center gap-1 px-3">
        <span v-if="source" class="ml-3 text-[13px] text-faint">{{ t('translate.chars', { n: source.length }) }}</span>
        <div class="flex-1" />
        <Tip v-if="source" :text="t('common.clear')">
          <Button variant="ghost" size="icon" @click="clear"><X class="size-5" /></Button>
        </Tip>
        <Tip text="Ctrl+Enter">
          <Button variant="primary" :disabled="!source.trim() || !llms.length || busy" @click="check">
            <SpellCheck class="size-[18px]" />
            {{ t('check.run') }}
          </Button>
        </Tip>
      </div>
    </section>

    <!-- result -->
    <section class="flex min-h-0 flex-col bg-surface">
      <div class="flex h-14 shrink-0 items-center gap-2 border-b border-line px-3">
        <Select v-model="level" :options="levelOptions" size="sm" :disabled="!llms.length" />
        <div class="flex-1" />
        <Tip v-if="state.status === 'done' && hasEdits" :text="showEdits ? t('check.hideEdits') : t('check.showEdits')">
          <Button variant="ghost" size="icon-sm" @click="showEdits = !showEdits">
            <EyeOff v-if="showEdits" class="size-[18px]" />
            <Eye v-else class="size-[18px]" />
          </Button>
        </Tip>
        <Select
          v-if="llms.length"
          v-model="providerId"
          :options="providerOptions"
          size="sm"
          trigger-class="max-w-48"
        />
      </div>

      <div class="min-h-0 flex-1 overflow-y-auto px-6 py-5">
        <!-- no AI provider -->
        <div v-if="!llms.length" class="flex h-full flex-col items-center justify-center px-6 text-center">
          <div class="flex size-14 items-center justify-center rounded-2xl bg-accent/10">
            <Sparkles class="size-7 text-accent" />
          </div>
          <h3 class="mt-4 text-[17px] font-semibold">{{ t('check.needsAi') }}</h3>
          <p class="mt-1.5 max-w-sm text-[14px] leading-relaxed text-muted">{{ t('check.needsAiDesc') }}</p>
          <Button variant="primary" class="mt-5" @click="navigate('settings/providers')">
            {{ t('check.connectAi') }}
          </Button>
        </div>

        <div v-else-if="state.status === 'error'" class="selectable rounded-2xl bg-danger/10 p-4 text-[14px] text-danger wrap-anywhere">
          {{ state.error }}
          <div class="mt-3 flex gap-2">
            <Button size="sm" @click="check">{{ t('common.retry') }}</Button>
            <Button v-if="setupError" size="sm" variant="ghost" @click="navigate('settings/providers')">
              {{ t('check.openProviders') }}
            </Button>
          </div>
        </div>

        <div v-else-if="busy" class="space-y-3" :aria-label="t('check.checking')">
          <div class="text-[13px] font-medium text-accent">{{ t('check.checking') }}</div>
          <div v-for="w in [92, 78, 85, 40]" :key="w" class="h-4 animate-pulse rounded-md bg-hover" :style="{ width: `${w}%` }" />
          <div class="!mt-6 h-20 animate-pulse rounded-2xl bg-hover" />
          <div class="h-20 animate-pulse rounded-2xl bg-hover" />
        </div>

        <template v-else-if="state.status === 'done' && state.result">
          <div
            v-if="stale"
            class="mb-4 flex items-center gap-3 rounded-xl bg-nav-selected px-3.5 py-2 text-[13px] text-muted"
          >
            <span class="flex-1">{{ t('check.stale') }}</span>
            <Button size="sm" variant="soft" @click="check">{{ t('check.recheck') }}</Button>
          </div>
          <CheckResultView v-model:accepted="state.accepted" :result="state.result" :show-edits="showEdits" />
        </template>

        <div v-else class="max-w-md text-[15px] leading-relaxed text-faint">
          <p>{{ t('check.empty') }}</p>
          <p class="mt-3">{{ t(`check.levelDesc.${level}`) }}</p>
        </div>
      </div>

      <div class="flex h-14 shrink-0 items-center gap-2 px-3">
        <span class="ml-3 min-w-0 flex-1 truncate text-[12.5px] text-faint">{{ meta.join(' · ') }}</span>
        <template v-if="state.status === 'done' && hasEdits">
          <Tip :text="copied ? t('common.copied') : t('common.copy')">
            <Button variant="soft" size="icon" @click="copy">
              <Check v-if="copied" class="size-5 text-success" />
              <Copy v-else class="size-5" />
            </Button>
          </Tip>
          <Button @click="apply">
            <Check class="size-5" />
            {{ t('check.apply') }}
          </Button>
        </template>
      </div>
    </section>
  </div>
</template>
