<script setup lang="ts">
import { onMounted, onBeforeUnmount, ref, watch } from 'vue';

/**
 * Windows 11 style selection bar for a vertical list: a short accent line
 * next to the active item. When the selection moves, the leading edge goes
 * first and the trailing edge follows, so the bar stretches and settles.
 *
 * The parent must be `position: relative`; `items` are the list elements.
 */
const props = defineProps<{ items: (HTMLElement | null)[]; active: number }>();

const BAR = 16;
const top = ref(0);
const bottom = ref(0);
const transition = ref('none');
let prev = -1;
let observer: ResizeObserver | null = null;

function edges(i: number) {
  const el = props.items[i];
  const parent = el?.offsetParent as HTMLElement | null;
  if (!el || !parent) return null;
  const t = el.offsetTop + (el.offsetHeight - BAR) / 2;
  return { top: t, bottom: parent.clientHeight - t - BAR };
}

function update(animate: boolean) {
  const e = edges(props.active);
  if (!e) return;
  if (animate && prev !== -1 && prev !== props.active) {
    const down = props.active > prev;
    const lead = '180ms cubic-bezier(0.3, 0, 0.2, 1)';
    const trail = '220ms cubic-bezier(0.3, 0, 0.2, 1) 110ms';
    transition.value = down ? `bottom ${lead}, top ${trail}` : `top ${lead}, bottom ${trail}`;
  } else {
    transition.value = 'none';
  }
  top.value = e.top;
  bottom.value = e.bottom;
  prev = props.active;
}

watch(() => props.active, () => update(true));
onMounted(() => {
  update(false);
  const parent = props.items[0]?.offsetParent;
  if (parent) {
    observer = new ResizeObserver(() => update(false));
    observer.observe(parent);
  }
});
onBeforeUnmount(() => observer?.disconnect());
</script>

<template>
  <span
    aria-hidden="true"
    class="pointer-events-none absolute left-1.5 w-[3px] rounded-full bg-accent"
    :style="{ top: `${top}px`, bottom: `${bottom}px`, transition }"
  />
</template>
