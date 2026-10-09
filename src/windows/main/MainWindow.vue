<script setup lang="ts">
import { onMounted, ref, type Component } from 'vue';
import { TooltipProvider } from 'reka-ui';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { History, Languages, Minus, Settings, Square, Copy, X } from '@lucide/vue';
import { useI18n } from 'vue-i18n';
import { events } from '@/lib/api';
import { autoSaveSettings } from '@/lib/settings';
import { dragWindow } from '@/lib/window-drag';
import Wordmark from '@/ui/Wordmark.vue';
import ResizeHandles from '@/ui/ResizeHandles.vue';
import TranslateView from './TranslateView.vue';
import HistoryView from './HistoryView.vue';
import SettingsView from './settings/SettingsView.vue';

type View = 'translate' | 'history' | 'settings';

const { t } = useI18n();
const win = getCurrentWindow();
const view = ref<View>('translate');
const maximized = ref(false);
const views: Record<View, Component> = { translate: TranslateView, history: HistoryView, settings: SettingsView };
const nav: { id: View; icon: Component }[] = [
  { id: 'translate', icon: Languages },
  { id: 'history', icon: History },
  { id: 'settings', icon: Settings },
];

autoSaveSettings();

onMounted(async () => {
  events.mainNavigate.listen((e) => {
    if (e.payload in views) view.value = e.payload as View;
  });
  maximized.value = await win.isMaximized();
  await win.onResized(async () => (maximized.value = await win.isMaximized()));
});
</script>

<template>
  <TooltipProvider>
    <div class="flex h-full flex-col">
      <ResizeHandles v-if="!maximized" />

      <!-- top bar -->
      <header
        class="grid h-16 shrink-0 grid-cols-[1fr_auto_1fr] items-center pl-5"
        @mousedown="dragWindow($event, { maximizeOnDoubleClick: true })"
      >
        <Wordmark class="text-[24px]" />

        <nav class="flex items-center gap-1 rounded-2xl border border-line bg-surface p-1">
          <button
            v-for="n in nav"
            :key="n.id"
            :class="[
              'flex h-10 items-center gap-2 rounded-xl px-4 text-[14px] font-semibold transition-colors duration-150',
              view === n.id
                ? 'bg-accent text-on-accent'
                : 'text-muted hover:bg-hover hover:text-fg',
            ]"
            @click="view = n.id"
          >
            <component :is="n.icon" class="size-[18px]" />
            {{ t(`nav.${n.id}`) }}
          </button>
        </nav>

        <div class="flex items-center justify-end gap-1 pr-3">
          <button
            class="flex size-9 items-center justify-center rounded-xl text-muted transition-colors hover:bg-hover hover:text-fg"
            :title="t('common.minimize')"
            @click="win.minimize()"
          >
            <Minus class="size-[18px]" />
          </button>
          <button
            class="flex size-9 items-center justify-center rounded-xl text-muted transition-colors hover:bg-hover hover:text-fg"
            :title="t('common.maximize')"
            @click="win.toggleMaximize()"
          >
            <Copy v-if="maximized" class="size-4 -scale-x-100" />
            <Square v-else class="size-[15px]" />
          </button>
          <button
            class="flex size-9 items-center justify-center rounded-xl text-muted transition-colors hover:bg-danger hover:text-white"
            :title="t('common.close')"
            @click="win.close()"
          >
            <X class="size-[18px]" />
          </button>
        </div>
      </header>

      <!-- Scrollable views (history, settings) run to the window edge; the translator keeps a margin. -->
      <main :class="['min-h-0 flex-1 pl-5', view === 'translate' && 'pb-5 pr-5']">
        <KeepAlive>
          <component :is="views[view]" />
        </KeepAlive>
      </main>
    </div>
  </TooltipProvider>
</template>
