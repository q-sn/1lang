<script setup lang="ts">
import { onMounted, ref } from 'vue';
import { Languages } from '@lucide/vue';
import { commands, events } from '@/lib/api';
import Spinner from '@/ui/Spinner.vue';

const busy = ref(false);

onMounted(() => {
  events.iconBusy.listen((e) => (busy.value = e.payload));
});
</script>

<template>
  <!-- Always dark, like a small floating toolbar, so it stands out on any page. -->
  <div class="flex h-full items-center justify-center p-1">
    <button
      class="flex size-full items-center justify-center rounded-[14px] border border-white/10 bg-[#33353b] text-white outline-none
        transition-[background-color,transform] duration-100 hover:bg-[#3d3f46] active:scale-95 animate-pop-in"
      @mousedown.prevent="!busy && commands.iconClicked()"
    >
      <Spinner v-if="busy" class="size-[46%] text-[#7aa2ff]" />
      <Languages v-else class="size-[48%]" stroke-width="2" />
    </button>
  </div>
</template>
