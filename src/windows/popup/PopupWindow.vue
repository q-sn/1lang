<script setup lang="ts">
import { computed, nextTick, onMounted, ref } from 'vue';
import {
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuPortal,
  DropdownMenuRoot,
  DropdownMenuTrigger,
  TooltipProvider,
} from 'reka-ui';
import {
  ArrowRight,
  Check,
  Copy,
  Ellipsis,
  EyeOff,
  Maximize2,
  Pin,
  PinOff,
  Replace,
  Settings,
  Volume2,
  X,
} from '@lucide/vue';
import { useI18n } from 'vue-i18n';
import { commands, events, type PopupPayload } from '@/lib/api';
import { useLanguages } from '@/lib/langs';
import { formatCost, speak, useTranslator } from '@/lib/translator';
import { useSettings } from '@/lib/settings';
import Button from '@/ui/Button.vue';
import DictionaryCard from '@/ui/DictionaryCard.vue';
import LanguagePicker from '@/ui/LanguagePicker.vue';
import Spinner from '@/ui/Spinner.vue';
import Tip from '@/ui/Tip.vue';
import ResizeHandles from '@/ui/ResizeHandles.vue';
import { dragWindow } from '@/lib/window-drag';
import { popupWidth, usePopupAutosize } from '@/lib/popup-autosize';

const { t } = useI18n();
const { name } = useLanguages();
const { settings } = useSettings();
const { state, run, reset } = useTranslator();

const payload = ref<PopupPayload | null>(null);
const source = ref('');
const editing = ref(false);
const expandedSource = ref(false);
const targetLang = ref<string | null>(null);
const pinned = ref(false);
const copied = ref(false);
const editor = ref<HTMLTextAreaElement | null>(null);
const rootEl = ref<HTMLElement | null>(null);
const scrollerEl = ref<HTMLElement | null>(null);
const contentEl = ref<HTMLElement | null>(null);

const autosize = usePopupAutosize(rootEl, scrollerEl, contentEl, [
  () => state.text,
  () => state.status,
  () => state.dictionary,
  () => payload.value?.id,
  editing,
  expandedSource,
], () => popupWidth(Math.max(source.value.length, state.text.length), settings.value.popup.width));

function show(p: PopupPayload) {
  payload.value = p;
  autosize.reset();
  source.value = p.text;
  targetLang.value = null;
  expandedSource.value = false;
  copied.value = false;
  if (settings.value.popup.always_pinned) pinned.value = true;
  reset();
  editing.value = !p.text;
  if (p.translate && p.text.trim()) translate();
  else if (editing.value) nextTick(() => editor.value?.focus());
}

async function translate() {
  if (!payload.value || !source.value.trim()) return;
  editing.value = false;
  await run({
    text: source.value,
    targetLang: targetLang.value,
    context: payload.value.context,
    origin: payload.value.origin,
    appExe: payload.value.app,
    saveHistory: true,
  });
}

function retarget(code: string | null) {
  targetLang.value = code;
  translate();
}

async function copy() {
  if (!state.text) return;
  await commands.copyText(state.text);
  copied.value = true;
  setTimeout(() => (copied.value = false), 1200);
}

function togglePin() {
  pinned.value = !pinned.value;
  commands.popupSetPinned(pinned.value);
}

function onKeydown(e: KeyboardEvent) {
  if (e.key === 'Escape') {
    e.preventDefault();
    commands.popupHide();
  } else if (e.key === 'Enter' && e.ctrlKey) {
    e.preventDefault();
    translate();
  } else if (e.key.toLowerCase() === 'c' && e.ctrlKey && !window.getSelection()?.toString() && !editing.value) {
    e.preventDefault();
    copy();
  }
}

const notice = computed(() => {
  const n = payload.value?.notice;
  if (!n) return null;
  if (n === 'no_selection') return t('popup.noSelection');
  if (n === 'ocr_empty') return t('popup.ocrEmpty');
  return n;
});

const meta = computed(() => {
  const r = state.result;
  if (!r) return state.providerName ?? '';
  if (r.cached) return `${r.provider_name} · ${t('translate.cached')}`;
  const cost = r.cost_usd ? ` · ${formatCost(r.cost_usd)}` : '';
  return `${r.provider_name} · ${t('common.ms', { n: r.elapsed_ms })}${cost}`;
});
const busy = computed(() => state.status === 'loading' || state.status === 'streaming');

onMounted(async () => {
  window.addEventListener('keydown', onKeydown);
  await events.popupShow.listen((e) => show(e.payload));
  await events.popupHidden.listen(() => {
    reset();
    pinned.value = false;
    window.speechSynthesis?.cancel();
  });
  const pending = await commands.popupTakePending();
  if (pending) show(pending);
});
</script>

<template>
  <TooltipProvider>
    <div ref="rootEl" class="relative flex h-full flex-col px-3 pb-3 pt-1 text-fg">
      <ResizeHandles @start="autosize.manual.value = true" />

      <header class="flex h-12 shrink-0 items-center gap-2" @mousedown="dragWindow">
        <div class="flex min-w-0 items-center gap-1.5 pl-1">
          <span class="pointer-events-none truncate text-[15px] text-muted">{{ name(state.sourceLang) || '…' }}</span>
          <ArrowRight class="pointer-events-none size-[18px] shrink-0 text-faint" />
          <LanguagePicker
            :model-value="targetLang"
            allow-auto
            :auto-label="t('lang.autoTarget')"
            :detected="targetLang ? null : state.targetLang"
            detected-key="lang.autoTo"
            variant="outline"
            @update:model-value="retarget"
          />
        </div>
        <div class="h-full min-w-2 flex-1" />
        <Tip :text="pinned ? t('popup.unpin') : t('popup.pin')">
          <Button variant="ghost" size="icon" :active="pinned" @click="togglePin">
            <PinOff v-if="pinned" class="size-5" />
            <Pin v-else class="size-5" />
          </Button>
        </Tip>
        <DropdownMenuRoot>
          <DropdownMenuTrigger as-child>
            <Button variant="ghost" size="icon"><Ellipsis class="size-5" /></Button>
          </DropdownMenuTrigger>
          <DropdownMenuPortal>
            <DropdownMenuContent
              :side-offset="6"
              :collision-padding="12"
              align="end"
              class="menu z-50 max-h-[var(--reka-dropdown-menu-content-available-height)] min-w-64 overflow-y-auto p-1.5 animate-pop-in"
            >
              <DropdownMenuItem
                class="flex h-11 items-center gap-3 rounded-[10px] px-3 text-[15px] outline-none data-[highlighted]:bg-hover"
                @select="speak(state.text, state.targetLang)"
              >
                <Volume2 class="size-5 text-muted" />{{ t('translate.speak') }}
              </DropdownMenuItem>
              <DropdownMenuItem
                v-if="payload?.app"
                class="flex h-11 items-center gap-3 rounded-[10px] px-3 text-[15px] outline-none data-[highlighted]:bg-hover"
                @select="commands.excludeApp(payload!.app!)"
              >
                <EyeOff class="size-5 text-muted" />{{ t('popup.exclude', { app: payload.app }) }}
              </DropdownMenuItem>
              <DropdownMenuItem
                class="flex h-11 items-center gap-3 rounded-[10px] px-3 text-[15px] outline-none data-[highlighted]:bg-hover"
                @select="commands.openMain('settings')"
              >
                <Settings class="size-5 text-muted" />{{ t('popup.settings') }}
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenuPortal>
        </DropdownMenuRoot>
        <Button variant="ghost" size="icon" :title="t('common.close')" @click="commands.popupHide()">
          <X class="size-5" />
        </Button>
      </header>

      <!-- original -->
      <div class="mt-1 shrink-0">
        <textarea
          v-if="editing"
          ref="editor"
          v-model="source"
          rows="3"
          spellcheck="false"
          class="w-full resize-none rounded-xl border border-line bg-surface px-4 py-3 text-[16px] outline-none focus:border-line-strong"
          @keydown.enter.ctrl.prevent="translate"
        />
        <button
          v-else-if="source"
          :class="[
            'selectable w-full rounded-xl px-1 py-1 text-left text-[15px] text-muted transition-colors hover:text-fg',
            expandedSource ? 'max-h-32 overflow-y-auto whitespace-pre-wrap' : 'line-clamp-2',
          ]"
          :title="t('popup.edit')"
          @click="expandedSource ? (editing = true) : (expandedSource = true)"
        >{{ source }}</button>
      </div>

      <!-- translation -->
      <div
        ref="scrollerEl"
        class="mt-2 min-h-0 flex-1 overflow-y-auto rounded-xl border border-line bg-surface px-4 py-3.5"
      >
        <div ref="contentEl">
          <div v-if="notice && state.status === 'idle'" class="text-[16px] text-muted">{{ notice }}</div>
          <div v-else-if="state.status === 'error'" class="selectable text-[15px] text-danger">
            {{ state.error }}
            <div class="mt-3"><Button size="sm" @click="translate">{{ t('common.retry') }}</Button></div>
          </div>
          <DictionaryCard
            v-else-if="state.kind === 'dictionary' && state.dictionary"
            :entry="state.dictionary"
            :source-lang="state.sourceLang"
          />
          <div v-else-if="state.text" :class="['result-text selectable', state.status === 'streaming' && 'stream-caret']">{{ state.text }}</div>
          <div v-else-if="busy" class="flex items-center justify-center py-3 text-accent">
            <Spinner class="size-7" />
          </div>
          <div v-else-if="editing" class="text-[15px] text-faint">Ctrl+Enter — {{ t('popup.translate') }}</div>
        </div>
      </div>

      <!-- actions -->
      <footer class="mt-3 flex shrink-0 items-center gap-2">
        <template v-if="editing">
          <span class="min-w-0 flex-1" />
          <Button variant="primary" :disabled="!source.trim()" @click="translate">{{ t('popup.translate') }}</Button>
        </template>
        <template v-else>
          <Button :disabled="!state.text" @click="copy">
            <Check v-if="copied" class="size-5 text-success" />
            <Copy v-else class="size-5" />
            {{ copied ? t('common.copied') : t('common.copy') }}
          </Button>
          <Button @click="commands.openInMain(source)">
            <Maximize2 class="size-5" />
            {{ t('popup.open') }}
          </Button>
          <span class="pointer-events-none min-w-0 flex-1 truncate text-center text-[12.5px] text-faint">{{ meta }}</span>
          <Button
            v-if="payload?.can_replace"
            variant="primary"
            :disabled="state.status !== 'done' || state.kind !== 'text'"
            @click="commands.replaceWith(state.text)"
          >
            <Replace class="size-5" />
            {{ t('popup.replaceShort') }}
          </Button>
        </template>
      </footer>

      <!-- resize grip -->
      <svg class="pointer-events-none absolute bottom-1 right-1 size-4 text-faint" viewBox="0 0 12 12" aria-hidden="true">
        <circle cx="10" cy="10" r="1.2" fill="currentColor" />
        <circle cx="6" cy="10" r="1.2" fill="currentColor" />
        <circle cx="10" cy="6" r="1.2" fill="currentColor" />
      </svg>
    </div>
  </TooltipProvider>
</template>
