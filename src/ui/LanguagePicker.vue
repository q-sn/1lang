<script setup lang="ts">
import { computed, nextTick, ref, watch } from 'vue';
import { PopoverContent, PopoverPortal, PopoverRoot, PopoverTrigger } from 'reka-ui';
import { Check, ChevronDown, Search } from '@lucide/vue';
import { useI18n } from 'vue-i18n';
import { useLanguages } from '@/lib/langs';

/** `null` = automatic (detect / auto-direction). */
const model = defineModel<string | null>({ required: true });
const props = defineProps<{
  allowAuto?: boolean;
  autoLabel?: string;
  /** Shown next to "auto" once known. */
  detected?: string | null;
  size?: 'sm' | 'md';
  align?: 'start' | 'end';
  variant?: 'ghost' | 'outline';
  /** i18n key used to show the detected language ("{lang} (detected)"). */
  detectedKey?: string;
}>();

const { t } = useI18n();
const { name, ordered } = useLanguages();
const open = ref(false);
const query = ref('');
const active = ref(0);
const input = ref<HTMLInputElement | null>(null);
const list = ref<HTMLElement | null>(null);

type Row = { kind: 'item'; code: string | null; label: string } | { kind: 'label'; label: string };

const rows = computed<Row[]>(() => {
  const q = query.value.trim().toLowerCase();
  const match = (c: string) => !q || name(c).toLowerCase().includes(q) || c.startsWith(q);
  const out: Row[] = [];
  if (props.allowAuto && !q) out.push({ kind: 'item', code: null, label: props.autoLabel ?? t('lang.auto') });
  const fav = ordered.value.favorites.filter(match);
  const rest = ordered.value.rest.filter(match);
  if (fav.length) {
    if (!q) out.push({ kind: 'label', label: t('lang.favorites') });
    fav.forEach((c) => out.push({ kind: 'item', code: c, label: name(c) }));
  }
  if (rest.length) {
    if (!q) out.push({ kind: 'label', label: t('lang.all') });
    rest.forEach((c) => out.push({ kind: 'item', code: c, label: name(c) }));
  }
  return out;
});

const items = computed(() => rows.value.filter((r) => r.kind === 'item') as Extract<Row, { kind: 'item' }>[]);

const triggerLabel = computed(() => {
  if (model.value) return name(model.value);
  if (props.detected) return t(props.detectedKey ?? 'lang.detected', { lang: name(props.detected) });
  return props.autoLabel ?? t('lang.auto');
});

watch(open, async (o) => {
  if (!o) return;
  query.value = '';
  active.value = Math.max(0, items.value.findIndex((i) => i.code === model.value));
  await nextTick();
  input.value?.focus();
  list.value?.querySelector('[data-active="true"]')?.scrollIntoView({ block: 'center' });
});
watch(query, () => (active.value = 0));

function pick(code: string | null) {
  model.value = code;
  open.value = false;
}

function onKey(e: KeyboardEvent) {
  if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
    e.preventDefault();
    const n = items.value.length;
    active.value = (active.value + (e.key === 'ArrowDown' ? 1 : -1) + n) % n;
    nextTick(() => list.value?.querySelector('[data-active="true"]')?.scrollIntoView({ block: 'nearest' }));
  } else if (e.key === 'Enter') {
    e.preventDefault();
    const it = items.value[active.value];
    if (it) pick(it.code);
  }
}

function indexOf(code: string | null) {
  return items.value.findIndex((i) => i.code === code);
}
</script>

<template>
  <PopoverRoot v-model:open="open">
    <PopoverTrigger
      :class="[
        'inline-flex min-w-0 items-center gap-1.5 font-semibold transition-colors',
        size === 'sm' ? 'h-8 rounded-[10px] px-2.5 text-[13px]' : 'h-10 rounded-xl px-3.5 text-[15px]',
        variant === 'outline' ? 'border border-line bg-surface-strong hover:bg-hover' : 'hover:bg-hover',
        'data-[state=open]:bg-hover',
      ]"
    >
      <span class="truncate">{{ triggerLabel }}</span>
      <ChevronDown class="size-4 shrink-0 text-faint" />
    </PopoverTrigger>
    <PopoverPortal>
      <PopoverContent
        :collision-padding="12"
        :side-offset="4"
        :align="align ?? 'start'"
        class="menu z-50 flex max-h-[var(--reka-popover-content-available-height)] w-72 flex-col overflow-hidden animate-pop-in"
        @keydown="onKey"
      >
        <div class="flex items-center gap-2.5 border-b border-line px-3.5">
          <Search class="size-4 text-faint" />
          <input
            ref="input"
            v-model="query"
            :placeholder="t('lang.search')"
            class="h-11 w-full bg-transparent text-[14px] outline-none placeholder:text-faint"
          />
        </div>
        <div ref="list" class="min-h-0 max-h-96 flex-1 overflow-y-auto p-1.5">
          <template v-for="(r, i) in rows" :key="i">
            <div v-if="r.kind === 'label'" class="px-3 pb-1.5 pt-2.5 text-[11.5px] font-semibold uppercase tracking-wider text-faint">
              {{ r.label }}
            </div>
            <button
              v-else
              :data-active="indexOf(r.code) === active"
              class="flex h-10 w-full items-center gap-2 rounded-[10px] px-3 text-left text-[14px] data-[active=true]:bg-hover"
              @click="pick(r.code)"
              @mousemove="active = indexOf(r.code)"
            >
              <span class="flex-1 truncate">{{ r.label }}</span>
              <Check v-if="r.code === model" class="size-4 text-accent" />
            </button>
          </template>
        </div>
      </PopoverContent>
    </PopoverPortal>
  </PopoverRoot>
</template>
