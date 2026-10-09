<script setup lang="ts">
import { getCurrentWindow } from '@tauri-apps/api/window';

/** Invisible edges / corners that resize an undecorated window. */
type Dir = 'North' | 'South' | 'East' | 'West' | 'NorthEast' | 'NorthWest' | 'SouthEast' | 'SouthWest';

const handles: { dir: Dir; class: string }[] = [
  { dir: 'North', class: 'top-0 left-3 right-3 h-1.5 cursor-ns-resize' },
  { dir: 'South', class: 'bottom-0 left-3 right-3 h-1.5 cursor-ns-resize' },
  { dir: 'West', class: 'left-0 top-3 bottom-3 w-1.5 cursor-ew-resize' },
  { dir: 'East', class: 'right-0 top-3 bottom-3 w-1.5 cursor-ew-resize' },
  { dir: 'NorthWest', class: 'top-0 left-0 size-3 cursor-nwse-resize' },
  { dir: 'SouthEast', class: 'bottom-0 right-0 size-5 cursor-nwse-resize' },
  { dir: 'NorthEast', class: 'top-0 right-0 size-3 cursor-nesw-resize' },
  { dir: 'SouthWest', class: 'bottom-0 left-0 size-3 cursor-nesw-resize' },
];

const emit = defineEmits<{ start: [] }>();

function start(e: MouseEvent, dir: Dir) {
  if (e.button !== 0) return;
  e.preventDefault();
  emit('start');
  getCurrentWindow().startResizeDragging(dir).catch(console.error);
}
</script>

<template>
  <div
    v-for="h in handles"
    :key="h.dir"
    data-no-drag
    :class="['fixed z-[100]', h.class]"
    @mousedown="start($event, h.dir)"
  />
</template>
