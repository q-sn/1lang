<script setup lang="ts">
import { computed, ref } from 'vue';
import { ChevronDown, ChevronUp, ChevronsUpDown, KeyRound, Trash2, Zap } from '@lucide/vue';
import { useI18n } from 'vue-i18n';
import { commands, unwrap, type ProviderConfig, type ProviderKindInfo } from '@/lib/api';
import { saveSettings } from '@/lib/settings';
import Button from '@/ui/Button.vue';
import ModelPicker from '@/ui/ModelPicker.vue';
import Spinner from '@/ui/Spinner.vue';
import Switch from '@/ui/Switch.vue';
import TextInput from '@/ui/TextInput.vue';
import Tip from '@/ui/Tip.vue';

const props = defineProps<{
  provider: ProviderConfig;
  info: ProviderKindInfo | undefined;
  keyHint: string | null;
  index: number;
  count: number;
}>();
const emit = defineEmits<{ move: [dir: -1 | 1]; remove: []; keyChanged: [] }>();

const { t } = useI18n();
const p = computed(() => props.provider);
const expanded = ref(false);
const keyInput = ref('');
const advanced = ref(false);
const testing = ref(false);
const testResult = ref<{ ok: boolean; text: string } | null>(null);
const confirmDelete = ref(false);

const isLlm = computed(() => props.info?.is_llm ?? true);
const needsKey = computed(() => props.info?.needs_key ?? true);
const hasBaseUrl = computed(() => isLlm.value || p.value.kind === 'deepl' || p.value.kind === 'microsoft');
const ready = computed(() => !needsKey.value || !!props.keyHint);

const KEY_URLS: Partial<Record<string, string>> = {
  groq: 'https://console.groq.com/keys',
  open_ai: 'https://platform.openai.com/api-keys',
  open_router: 'https://openrouter.ai/keys',
  gemini: 'https://aistudio.google.com/apikey',
  anthropic: 'https://console.anthropic.com/settings/keys',
  deepl: 'https://www.deepl.com/your-account/keys',
  google_cloud: 'https://console.cloud.google.com/apis/credentials',
  microsoft: 'https://portal.azure.com/',
};

// Nullable numeric fields bound to text inputs.
function numberModel(key: 'temperature' | 'price_input' | 'price_output') {
  return computed({
    get: () => (p.value[key] ?? '') as string | number,
    set: (v: string | number | null | undefined) => {
      const n = typeof v === 'number' ? v : parseFloat(String(v ?? '').replace(',', '.'));
      p.value[key] = Number.isFinite(n) ? n : null;
    },
  });
}
const temperature = numberModel('temperature');
const priceIn = numberModel('price_input');
const priceOut = numberModel('price_output');

function textModel(key: 'base_url' | 'region' | 'extra_body') {
  return computed({
    get: () => p.value[key] ?? '',
    set: (v: string | number | null | undefined) => {
      const s = String(v ?? '');
      p.value[key] = s.trim() ? s : null;
    },
  });
}
const baseUrl = textModel('base_url');
const modelId = computed({
  get: () => p.value.model ?? props.info?.default_model ?? '',
  set: (v: string) => (p.value.model = v.trim() || null),
});
const region = textModel('region');
const extraBody = textModel('extra_body');

async function saveKey() {
  await unwrap(commands.setApiKey(p.value.id, keyInput.value.trim() || null));
  keyInput.value = '';
  emit('keyChanged');
}

async function removeKey() {
  await unwrap(commands.setApiKey(p.value.id, null));
  emit('keyChanged');
}


async function test() {
  testing.value = true;
  testResult.value = null;
  try {
    // Make sure the latest edits are saved before testing.
    await saveSettings(true);
    const r = await unwrap(commands.testProvider(p.value.id));
    testResult.value = { ok: true, text: `${t('settings.providers.testOk', { ms: r.elapsed_ms })} — ${r.text}` };
  } catch (e) {
    testResult.value = { ok: false, text: e instanceof Error ? e.message : String(e) };
  } finally {
    testing.value = false;
  }
}

function askRemove() {
  if (confirmDelete.value) emit('remove');
  else {
    confirmDelete.value = true;
    setTimeout(() => (confirmDelete.value = false), 3000);
  }
}
</script>

<template>
  <div class="panel overflow-hidden">
    <div class="flex items-center gap-3 px-4 py-3.5">
      <Switch v-model="provider.enabled" />
      <button class="flex min-w-0 flex-1 items-center gap-2 text-left" @click="expanded = !expanded">
        <span class="w-5 text-center text-[12px] font-semibold text-faint">{{ index + 1 }}</span>
        <span class="truncate text-[15px] font-semibold">{{ provider.name }}</span>
        <span class="shrink-0 rounded bg-hover px-1.5 py-0.5 text-[11px] text-muted">
          {{ isLlm ? provider.model || info?.default_model : info?.label }}
        </span>
        <span
          v-if="needsKey"
          :class="[
            'inline-flex shrink-0 items-center gap-1 rounded px-1.5 py-0.5 text-[11px]',
            ready ? 'bg-success/10 text-success' : 'bg-warning/10 text-warning',
          ]"
        >
          <KeyRound class="size-3" />
          {{ ready ? keyHint : t('settings.providers.apiKeyMissing') }}
        </span>
      </button>
      <div class="flex items-center gap-0.5">
        <Tip :text="t('settings.providers.test')">
          <Button variant="ghost" size="icon-sm" :disabled="testing || !ready" @click="test">
            <Spinner v-if="testing" class="size-3.5" />
            <Zap v-else class="size-4" />
          </Button>
        </Tip>
        <Tip :text="t('common.moveUp')">
          <Button variant="ghost" size="icon-sm" :disabled="index === 0" @click="emit('move', -1)">
            <ChevronUp class="size-4" />
          </Button>
        </Tip>
        <Tip :text="t('common.moveDown')">
          <Button variant="ghost" size="icon-sm" :disabled="index === count - 1" @click="emit('move', 1)">
            <ChevronDown class="size-4" />
          </Button>
        </Tip>
        <Button variant="ghost" size="icon-sm" @click="expanded = !expanded">
          <ChevronsUpDown class="size-4" />
        </Button>
      </div>
    </div>

    <div
      v-if="testResult"
      :class="[
        'selectable mx-3 mb-2.5 rounded-md px-2.5 py-1.5 text-[12px]',
        testResult.ok ? 'bg-success/10 text-success' : 'bg-danger/10 text-danger',
      ]"
    >
      {{ testResult.text }}
    </div>

    <div v-if="expanded" class="grid grid-cols-2 gap-x-4 gap-y-4 border-t border-line px-5 py-5 text-[13px]">
      <label class="col-span-2 space-y-1">
        <span class="text-muted">{{ t('settings.providers.name') }}</span>
        <TextInput v-model="provider.name" />
      </label>

      <div v-if="needsKey || provider.kind === 'open_ai_compatible'" class="col-span-2 space-y-1">
        <div class="flex items-center justify-between">
          <span class="text-muted">{{ t('settings.providers.apiKey') }}</span>
          <a
            v-if="KEY_URLS[provider.kind]"
            :href="KEY_URLS[provider.kind]"
            target="_blank"
            class="text-[12px] text-accent hover:underline"
          >{{ KEY_URLS[provider.kind]!.replace('https://', '').split('/')[0] }}</a>
        </div>
        <div class="flex gap-2">
          <TextInput
            v-model="keyInput"
            type="password"
            mono
            :placeholder="keyHint ? t('settings.providers.apiKeySet', { hint: keyHint }) : 'sk-…'"
            @keydown.enter="saveKey"
          />
          <Button variant="primary" :disabled="!keyInput.trim()" @click="saveKey">{{ t('settings.providers.saveKey') }}</Button>
          <Button v-if="keyHint" variant="ghost" @click="removeKey">{{ t('settings.providers.removeKey') }}</Button>
        </div>
      </div>
      <div v-else class="col-span-2 text-muted">{{ t('settings.providers.noKeyNeeded') }}</div>

      <label v-if="hasBaseUrl" class="col-span-2 space-y-1">
        <span class="text-muted">{{ t('settings.providers.baseUrl') }}</span>
        <TextInput v-model="baseUrl" mono :placeholder="info?.default_base_url ?? ''" />
      </label>

      <template v-if="isLlm">
        <div class="col-span-2 space-y-1">
          <span class="text-muted">{{ t('settings.providers.model') }}</span>
          <ModelPicker
            v-model="modelId"
            :provider-id="provider.id"
            :recommended="info?.recommended_models ?? []"
            :can-load="ready"
          />
          <p class="text-[12.5px] text-faint">{{ t('settings.providers.modelHint') }}</p>
        </div>
        <button
          class="col-span-2 flex items-center gap-1.5 text-left text-[13px] font-medium text-muted hover:text-fg"
          @click="advanced = !advanced"
        >
          <ChevronDown :class="['size-4 transition-transform', advanced && 'rotate-180']" />
          {{ t('settings.providers.advanced') }}
        </button>
        <template v-if="advanced">
        <label class="space-y-1">
          <span class="text-muted">{{ t('settings.providers.priceIn') }}</span>
          <TextInput v-model="priceIn" placeholder="auto" />
        </label>
        <label class="space-y-1">
          <span class="text-muted">{{ t('settings.providers.priceOut') }}</span>
          <TextInput v-model="priceOut" placeholder="auto" />
        </label>
        <label class="space-y-1">
          <span class="text-muted">{{ t('settings.providers.temperature') }}</span>
          <TextInput v-model="temperature" placeholder="0.2" />
        </label>
        <div />
        <label class="col-span-2 space-y-1">
          <span class="text-muted">{{ t('settings.providers.extraBody') }}</span>
          <textarea
            v-model="extraBody"
            rows="2"
            spellcheck="false"
            class="w-full resize-y rounded-md border border-line bg-surface-strong px-2.5 py-1.5 font-mono text-[12px] outline-none focus:border-line-strong"
          />
          <span class="text-[11.5px] text-faint">{{ t('settings.providers.extraBodyHint') }}</span>
        </label>
        </template>
      </template>

      <template v-else>
        <label v-if="provider.kind === 'microsoft'" class="space-y-1">
          <span class="text-muted">{{ t('settings.providers.region') }}</span>
          <TextInput v-model="region" placeholder="westeurope" />
        </label>
        <label v-if="provider.kind !== 'google_free'" class="space-y-1">
          <span class="text-muted">{{ t('settings.providers.priceChars') }}</span>
          <TextInput v-model="priceIn" placeholder="auto" />
        </label>
      </template>

      <div class="col-span-2 flex justify-end">
        <Button variant="danger" size="sm" @click="askRemove">
          <Trash2 class="size-3.5" />
          {{ confirmDelete ? t('settings.providers.deleteConfirm', { name: provider.name }) : t('common.delete') }}
        </Button>
      </div>
    </div>
  </div>
</template>
