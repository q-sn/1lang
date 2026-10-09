<script setup lang="ts">
import { computed, ref, watch } from 'vue';
import { X } from '@lucide/vue';
import { useI18n } from 'vue-i18n';
import { useSettings } from '@/lib/settings';
import { useLanguages } from '@/lib/langs';
import Card from '@/ui/Card.vue';
import Field from '@/ui/Field.vue';
import LanguagePicker from '@/ui/LanguagePicker.vue';
import Segmented from '@/ui/Segmented.vue';

const { t } = useI18n();
const { settings } = useSettings();
const { name } = useLanguages();
const l = computed(() => settings.value.languages);

// LanguagePicker models are nullable; these settings are not.
const primary = computed({
  get: () => l.value.primary as string | null,
  set: (v) => v && (l.value.primary = v),
});
const secondary = computed({
  get: () => l.value.secondary as string | null,
  set: (v) => v && (l.value.secondary = v),
});

const adding = ref<string | null>(null);
watch(adding, (code) => {
  if (code && !l.value.favorites.includes(code)) l.value.favorites.push(code);
  adding.value = null;
});

function removeFavorite(code: string) {
  l.value.favorites = l.value.favorites.filter((c) => c !== code);
}

const detection = computed(() => [
  { value: 'local' as const, label: t('settings.languages.detectionLocal') },
  { value: 'llm' as const, label: t('settings.languages.detectionLlm') },
]);
</script>

<template>
  <div>
    <Card>
      <Field :label="t('settings.languages.primary')" :description="t('settings.languages.primaryDesc')">
        <LanguagePicker v-model="primary" variant="outline" align="end" />
      </Field>
      <Field :label="t('settings.languages.secondary')" :description="t('settings.languages.secondaryDesc')">
        <LanguagePicker v-model="secondary" variant="outline" align="end" />
      </Field>
      <div class="border-t border-line px-5 py-3 text-[13px] text-muted">
        {{ t('settings.languages.pair', { primary: name(l.primary), secondary: name(l.secondary) }) }}
      </div>
    </Card>

    <Card :title="t('settings.languages.favorites')" :description="t('settings.languages.favoritesDesc')">
      <div class="flex flex-wrap items-center gap-2 p-4">
        <span
          v-for="code in l.favorites"
          :key="code"
          class="inline-flex h-9 items-center gap-1 rounded-full border border-line bg-surface-strong pl-4 pr-1.5 text-[14px] font-medium"
        >
          {{ name(code) }}
          <button class="rounded-full p-0.5 text-muted hover:bg-hover hover:text-fg" @click="removeFavorite(code)">
            <X class="size-3.5" />
          </button>
        </span>
        <LanguagePicker v-model="adding" allow-auto :auto-label="'+ ' + t('settings.languages.addFavorite')" size="sm" />
      </div>
    </Card>

    <Card>
      <Field
        :label="t('settings.languages.detection')"
        :description="l.detection === 'local' ? t('settings.languages.detectionLocalDesc') : t('settings.languages.detectionLlmDesc')"
      >
        <Segmented v-model="l.detection" :options="detection" />
      </Field>
    </Card>
  </div>
</template>
