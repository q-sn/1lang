<script setup lang="ts">
import { ref, type Component } from 'vue';
import {
  BarChart3,
  Bot,
  Keyboard,
  Languages,
  MessageSquareText,
  MousePointerClick,
  PanelTop,
  ScanText,
  SlidersHorizontal,
} from '@lucide/vue';
import { useI18n } from 'vue-i18n';
import { useSettings } from '@/lib/settings';
import GeneralSection from './GeneralSection.vue';
import LanguagesSection from './LanguagesSection.vue';
import ProvidersSection from './ProvidersSection.vue';
import TranslationSection from './TranslationSection.vue';
import SelectionSection from './SelectionSection.vue';
import HotkeysSection from './HotkeysSection.vue';
import PopupSection from './PopupSection.vue';
import OcrSection from './OcrSection.vue';
import UsageSection from './UsageSection.vue';
import NavIndicator from '@/ui/NavIndicator.vue';

const { t } = useI18n();
const { warnings } = useSettings();

const sections: { id: string; component: Component; icon: Component }[] = [
  { id: 'general', icon: SlidersHorizontal, component: GeneralSection },
  { id: 'languages', icon: Languages, component: LanguagesSection },
  { id: 'providers', icon: Bot, component: ProvidersSection },
  { id: 'translation', icon: MessageSquareText, component: TranslationSection },
  { id: 'selection', icon: MousePointerClick, component: SelectionSection },
  { id: 'hotkeys', icon: Keyboard, component: HotkeysSection },
  { id: 'popup', icon: PanelTop, component: PopupSection },
  { id: 'ocr', icon: ScanText, component: OcrSection },
  { id: 'usage', icon: BarChart3, component: UsageSection },
];
const current = ref('general');
const navItems = ref<(HTMLElement | null)[]>([]);
</script>

<template>
  <div class="flex h-full gap-5">
    <aside class="relative flex w-60 shrink-0 flex-col gap-1 overflow-y-auto">
      <button
        v-for="(s, i) in sections"
        :key="s.id"
        :ref="(el) => (navItems[i] = el as HTMLElement | null)"
        :class="[
          'flex h-11 shrink-0 items-center gap-3 rounded-xl px-3.5 text-left text-[14.5px] font-medium transition-colors',
          current === s.id ? 'bg-nav-selected text-fg' : 'text-muted hover:bg-hover hover:text-fg',
        ]"
        @click="current = s.id"
      >
        <component :is="s.icon" class="size-[18px] shrink-0" />
        <span class="truncate">{{ t(`settings.sections.${s.id}`) }}</span>
      </button>
      <NavIndicator :items="navItems" :active="sections.findIndex((s) => s.id === current)" />
    </aside>
    <div class="min-w-0 flex-1 overflow-y-auto pr-5">
      <div class="mx-auto max-w-[760px] pb-8">
        <h2 class="mb-5 px-1 text-[26px] font-bold tracking-tight">
          {{ t(`settings.sections.${current}`) }}
        </h2>
        <div
          v-for="w in warnings"
          :key="w"
          class="mb-4 rounded-2xl border border-warning/40 bg-warning/10 px-4 py-3 text-[13.5px] text-warning"
        >
          {{ w }}
        </div>
        <KeepAlive>
          <component :is="sections.find((s) => s.id === current)!.component" />
        </KeepAlive>
      </div>
    </div>
  </div>
</template>
