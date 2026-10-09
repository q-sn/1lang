<script setup lang="ts">
defineProps<{ title?: string; description?: string }>();
</script>

<template>
  <section class="mb-7">
    <h3 v-if="title" class="mb-1 px-1 text-[15px] font-semibold text-fg">{{ title }}</h3>
    <p v-if="description" class="mb-3 px-1 text-[13px] leading-snug text-muted">{{ description }}</p>
    <div class="panel overflow-hidden">
      <slot />
    </div>
  </section>
</template>
