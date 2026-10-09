<script setup lang="ts">
import { onActivated, onMounted, ref, watch } from 'vue';
import { useDebounceFn } from '@vueuse/core';
import { ArrowRight, Copy, History as HistoryIcon, Search, Star, Trash2, Upload } from '@lucide/vue';
import { useI18n } from 'vue-i18n';
import { commands, events, unwrap, type HistoryItem } from '@/lib/api';
import { useLanguages } from '@/lib/langs';
import Button from '@/ui/Button.vue';
import Segmented from '@/ui/Segmented.vue';
import Tip from '@/ui/Tip.vue';

const { t, locale } = useI18n();
const { name } = useLanguages();

const items = ref<HistoryItem[]>([]);
const search = ref('');
const filter = ref<'all' | 'favorites'>('all');
const confirmClear = ref(false);

async function load() {
  items.value = await unwrap(
    commands.historyList({
      search: search.value || null,
      favorites_only: filter.value === 'favorites',
      limit: 300,
      offset: 0,
    }),
  );
}

const reload = useDebounceFn(load, 200);
watch([search, filter], reload);
onMounted(() => {
  load();
  events.historyChanged.listen(() => reload());
});
onActivated(load);

async function toggleFavorite(it: HistoryItem) {
  await commands.historyToggleFavorite(it.id);
  it.favorite = !it.favorite;
  if (filter.value === 'favorites' && !it.favorite) load();
}

async function remove(it: HistoryItem) {
  await commands.historyDelete(it.id);
  items.value = items.value.filter((i) => i.id !== it.id);
}

async function clearAll() {
  if (!confirmClear.value) {
    confirmClear.value = true;
    setTimeout(() => (confirmClear.value = false), 3000);
    return;
  }
  confirmClear.value = false;
  await commands.historyClear(true);
  load();
}

function when(ms: number | null) {
  if (!ms) return '';
  const d = new Date(ms);
  const sameDay = d.toDateString() === new Date().toDateString();
  return new Intl.DateTimeFormat(locale.value, sameDay ? { timeStyle: 'short' } : { dateStyle: 'medium', timeStyle: 'short' }).format(d);
}
</script>

<template>
  <div class="flex h-full flex-col gap-4">
    <div class="flex items-center gap-3 pr-5">
      <div class="relative max-w-md flex-1">
        <Search class="absolute left-3.5 top-1/2 size-[18px] -translate-y-1/2 text-faint" />
        <input
          v-model="search"
          :placeholder="t('history.search')"
          class="h-11 w-full rounded-2xl border border-line bg-surface-strong pl-11 pr-4 text-[14.5px] outline-none
            placeholder:text-faint focus:border-line-strong"
        />
      </div>
      <Segmented
        v-model="filter"
        :options="[
          { value: 'all', label: t('history.all') },
          { value: 'favorites', label: t('history.favorites') },
        ]"
      />
      <div class="flex-1" />
      <Button variant="danger" @click="clearAll">
        <Trash2 class="size-4" />
        {{ confirmClear ? t('history.clearConfirm') : t('history.clear') }}
      </Button>
    </div>

    <div class="min-h-0 flex-1 space-y-2.5 overflow-y-auto pb-5 pr-5">
      <div v-if="!items.length" class="flex h-full flex-col items-center justify-center gap-3 text-faint">
        <HistoryIcon class="size-12 opacity-40" />
        <span class="text-[15px]">{{ t('history.empty') }}</span>
      </div>
      <article
        v-for="it in items"
        :key="it.id"
        class="panel group grid grid-cols-[1fr_1fr_auto] gap-5 px-5 py-4 transition-colors hover:bg-surface-strong"
      >
        <div class="min-w-0">
          <div class="mb-1.5 flex items-center gap-1.5 text-[12.5px] font-medium text-faint">
            <span>{{ name(it.source_lang) || '—' }}</span>
            <ArrowRight class="size-3.5" />
            <span class="text-accent">{{ name(it.target_lang) }}</span>
            <span class="ml-1 rounded-md bg-hover px-1.5 py-0.5 text-[11.5px]">{{ t(`history.origin.${it.origin}`) }}</span>
            <span v-if="it.app" class="truncate">{{ it.app }}</span>
          </div>
          <p class="selectable line-clamp-3 whitespace-pre-wrap text-[14.5px]">{{ it.source_text }}</p>
        </div>
        <div class="min-w-0">
          <div class="mb-1.5 text-[12.5px] text-faint">{{ when(it.created_at) }} · {{ it.provider_name }}</div>
          <p class="selectable line-clamp-3 whitespace-pre-wrap text-[14.5px] font-medium">{{ it.result_text }}</p>
        </div>
        <div class="flex items-start gap-0.5">
          <div class="flex gap-0.5 opacity-0 transition-opacity group-hover:opacity-100">
            <Tip :text="t('common.copy')">
              <Button variant="ghost" size="icon-sm" @click="commands.copyText(it.result_text)"><Copy class="size-4" /></Button>
            </Tip>
            <Tip :text="t('history.open')">
              <Button variant="ghost" size="icon-sm" @click="commands.openInMain(it.source_text)"><Upload class="size-4" /></Button>
            </Tip>
            <Tip :text="t('common.delete')">
              <Button variant="ghost" size="icon-sm" @click="remove(it)"><Trash2 class="size-4" /></Button>
            </Tip>
          </div>
          <Tip :text="t('history.favorite')">
            <Button variant="ghost" size="icon-sm" @click="toggleFavorite(it)">
              <Star :class="['size-[18px]', it.favorite && 'fill-[#f5b301] text-[#f5b301]']" />
            </Button>
          </Tip>
        </div>
      </article>
    </div>
  </div>
</template>
