<script setup lang="ts">
import { computed, onMounted, reactive, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { commands, events, type RegionShow } from '@/lib/api';

const { t } = useI18n();
const shot = ref<RegionShow | null>(null);
const loaded = ref(false);
const drag = reactive({ active: false, x0: 0, y0: 0, x1: 0, y1: 0 });
const mouse = reactive({ x: -100, y: -100 });

const rect = computed(() => ({
  x: Math.min(drag.x0, drag.x1),
  y: Math.min(drag.y0, drag.y1),
  w: Math.abs(drag.x1 - drag.x0),
  h: Math.abs(drag.y1 - drag.y0),
}));
const hasRect = computed(() => rect.value.w > 2 && rect.value.h > 2);

function load(p: RegionShow) {
  loaded.value = false;
  drag.active = false;
  drag.x0 = drag.x1 = drag.y0 = drag.y1 = 0;
  shot.value = p;
}

function onDown(e: PointerEvent) {
  if (e.button === 2) {
    commands.regionCancel();
    return;
  }
  if (e.button !== 0) return;
  (e.target as HTMLElement).setPointerCapture(e.pointerId);
  drag.active = true;
  drag.x0 = drag.x1 = e.clientX;
  drag.y0 = drag.y1 = e.clientY;
}

function onMove(e: PointerEvent) {
  mouse.x = e.clientX;
  mouse.y = e.clientY;
  if (drag.active) {
    drag.x1 = e.clientX;
    drag.y1 = e.clientY;
  }
}

function onUp() {
  if (!drag.active || !shot.value) return;
  drag.active = false;
  if (rect.value.w < 6 || rect.value.h < 6) return;
  const dpr = window.devicePixelRatio;
  commands.regionSelected(shot.value.index, {
    x: Math.round(rect.value.x * dpr),
    y: Math.round(rect.value.y * dpr),
    width: Math.round(rect.value.w * dpr),
    height: Math.round(rect.value.h * dpr),
  });
  shot.value = null;
}

onMounted(async () => {
  window.addEventListener('keydown', (e) => {
    if (e.key === 'Escape') commands.regionCancel();
  });
  await events.regionShow.listen((e) => load(e.payload));
  const p = await commands.regionInit();
  if (p) load(p);
});
</script>

<template>
  <div
    class="relative h-full w-full cursor-crosshair overflow-hidden bg-black"
    @pointerdown="onDown"
    @pointermove="onMove"
    @pointerup="onUp"
    @contextmenu.prevent
  >
    <img
      v-if="shot"
      :src="shot.url"
      class="pointer-events-none absolute inset-0 h-full w-full select-none"
      draggable="false"
      @load="(loaded = true), commands.regionReady(shot!.index)"
    />
    <!-- dim everything except the selection -->
    <div
      v-if="hasRect"
      class="pointer-events-none absolute rounded-[2px] outline outline-2 outline-white/90"
      :style="{
        left: `${rect.x}px`,
        top: `${rect.y}px`,
        width: `${rect.w}px`,
        height: `${rect.h}px`,
        boxShadow: '0 0 0 100vmax rgb(0 0 0 / 0.45)',
      }"
    >
      <span
        class="absolute -top-6 left-0 rounded bg-black/70 px-1.5 py-0.5 font-mono text-[11px] text-white"
      >{{ Math.round(rect.w * 1) }} × {{ Math.round(rect.h * 1) }}</span>
    </div>
    <div v-else class="pointer-events-none absolute inset-0 bg-black/35" />

    <!-- crosshair -->
    <template v-if="loaded && !drag.active">
      <div class="pointer-events-none absolute left-0 right-0 h-px bg-white/40" :style="{ top: `${mouse.y}px` }" />
      <div class="pointer-events-none absolute bottom-0 top-0 w-px bg-white/40" :style="{ left: `${mouse.x}px` }" />
    </template>

    <div
      v-if="loaded && !drag.active && !hasRect"
      class="pointer-events-none absolute left-1/2 top-8 -translate-x-1/2 rounded-full bg-black/65 px-4 py-2 text-[13px] text-white backdrop-blur"
    >
      {{ t('region.hint') }}
    </div>
  </div>
</template>
