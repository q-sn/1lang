<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue';
import { PopoverContent, PopoverPortal, PopoverRoot, PopoverTrigger } from 'reka-ui';
import { Check, ChevronDown, Search } from '@lucide/vue';
import { useI18n } from 'vue-i18n';
import { commands, unwrap, type ModelTier, type RecommendedModel } from '@/lib/api';
import Spinner from './Spinner.vue';

const model = defineModel<string>({ required: true });
const props = defineProps<{
  providerId: string;
  recommended: RecommendedModel[];
  /** Models can be listed (the key is set or not needed). */
  canLoad: boolean;
}>();

const { t } = useI18n();
const open = ref(false);
const query = ref('');
const input = ref<HTMLInputElement | null>(null);
const loaded = ref<string[] | null>(null);
const loading = ref(false);
const error = ref<string | null>(null);

async function load() {
  if (!props.canLoad || loading.value) return;
  loading.value = true;
  error.value = null;
  try {
    loaded.value = await unwrap(commands.listModels(props.providerId));
  } catch (e) {
    error.value = e instanceof Error ? e.message : String(e);
  } finally {
    loading.value = false;
  }
}

watch(open, async (o) => {
  if (!o) return;
  query.value = '';
  if (loaded.value === null) load();
  await nextTick();
  input.value?.focus();
});

const q = computed(() => query.value.trim().toLowerCase());
const tierOf = (id: string) => props.recommended.find((r) => r.id === id)?.tier;
const recommended = computed(() => props.recommended.filter((r) => !q.value || r.id.toLowerCase().includes(q.value)));
const others = computed(() =>
  (loaded.value ?? []).filter((id) => !tierOf(id) && (!q.value || id.toLowerCase().includes(q.value))),
);
const custom = computed(() => {
  const v = query.value.trim();
  if (!v) return null;
  const known = props.recommended.some((r) => r.id === v) || (loaded.value ?? []).includes(v);
  return known ? null : v;
});

function pick(id: string) {
  model.value = id;
  open.value = false;
}

const TIER_CLASS: Record<ModelTier, string> = {
  recommended: 'bg-accent text-on-accent',
  fast: 'bg-success/12 text-success',
  quality: 'bg-warning/12 text-warning',
};
</script>

<template>
  <PopoverRoot v-model:open="open">
    <PopoverTrigger
      class="flex h-10 w-full min-w-0 items-center gap-2 rounded-xl border border-line bg-surface-strong px-3.5 text-left
        transition-colors hover:bg-hover data-[state=open]:bg-hover"
    >
      <span class="truncate font-mono text-[13px]">{{ model }}</span>
      <span
        v-if="tierOf(model)"
        :class="['shrink-0 rounded-md px-1.5 py-0.5 text-[11.5px] font-semibold', TIER_CLASS[tierOf(model)!]]"
      >
        {{ t(`settings.providers.tiers.${tierOf(model)}`) }}
      </span>
      <span class="flex-1" />
      <ChevronDown class="size-4 shrink-0 text-faint" />
    </PopoverTrigger>
    <PopoverPortal>
      <PopoverContent
        :collision-padding="12"
        :side-offset="6"
        align="start"
        class="menu z-50 flex max-h-[var(--reka-popover-content-available-height)] w-[var(--reka-popover-trigger-width)] min-w-80 flex-col overflow-hidden animate-pop-in"
        @keydown.enter.prevent="custom ? pick(custom) : undefined"
      >
        <div class="flex items-center gap-2.5 border-b border-line px-3.5">
          <Search class="size-4 text-faint" />
          <input
            ref="input"
            v-model="query"
            :placeholder="t('settings.providers.modelSearch')"
            spellcheck="false"
            class="h-11 w-full bg-transparent font-mono text-[13px] outline-none placeholder:font-sans placeholder:text-faint"
          />
        </div>
        <div class="min-h-0 max-h-96 flex-1 overflow-y-auto p-1.5">
          <button
            v-if="custom"
            class="flex h-10 w-full items-center rounded-[10px] px-3 text-left text-[13.5px] hover:bg-hover"
            @click="pick(custom)"
          >
            {{ t('settings.providers.useModel', { id: custom }) }}
          </button>

          <template v-if="recommended.length">
            <div class="px-3 pb-1.5 pt-2.5 text-[11.5px] font-semibold uppercase tracking-wider text-faint">
              {{ t('settings.providers.recommendedModels') }}
            </div>
            <button
              v-for="r in recommended"
              :key="r.id"
              class="flex h-10 w-full items-center gap-2 rounded-[10px] px-3 text-left hover:bg-hover"
              @click="pick(r.id)"
            >
              <span class="truncate font-mono text-[13px]">{{ r.id }}</span>
              <span :class="['shrink-0 rounded-md px-1.5 py-0.5 text-[11.5px] font-semibold', TIER_CLASS[r.tier]]">
                {{ t(`settings.providers.tiers.${r.tier}`) }}
              </span>
              <span class="flex-1" />
              <Check v-if="r.id === model" class="size-4 shrink-0 text-accent" />
            </button>
          </template>

          <div class="flex items-center gap-2 px-3 pb-1.5 pt-2.5 text-[11.5px] font-semibold uppercase tracking-wider text-faint">
            {{ t('settings.providers.allModels') }}
            <Spinner v-if="loading" class="size-3.5" />
          </div>
          <div v-if="!canLoad" class="px-3 py-2 text-[13px] text-faint">{{ t('settings.providers.modelsNeedKey') }}</div>
          <div v-else-if="error" class="selectable px-3 py-2 text-[13px] text-danger">
            {{ error }}
            <button class="ml-1 underline" @click="load">{{ t('common.retry') }}</button>
          </div>
          <button
            v-for="id in others"
            :key="id"
            class="flex h-9 w-full items-center gap-2 rounded-[10px] px-3 text-left hover:bg-hover"
            @click="pick(id)"
          >
            <span class="truncate font-mono text-[13px]">{{ id }}</span>
            <span class="flex-1" />
            <Check v-if="id === model" class="size-4 shrink-0 text-accent" />
          </button>
        </div>
      </PopoverContent>
    </PopoverPortal>
  </PopoverRoot>
</template>
