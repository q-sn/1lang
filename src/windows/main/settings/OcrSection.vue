<script setup lang="ts">
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { useSettings } from '@/lib/settings';
import { languageName } from '@/lib/langs';
import Card from '@/ui/Card.vue';
import Field from '@/ui/Field.vue';
import Segmented from '@/ui/Segmented.vue';
import Select from '@/ui/Select.vue';
import Switch from '@/ui/Switch.vue';

const { t, locale } = useI18n();
const { settings, appInfo } = useSettings();
const o = computed(() => settings.value.ocr);
const installed = computed(() => appInfo.value?.ocr_languages ?? []);
const snippingInstalled = computed(() => appInfo.value?.snipping_ocr ?? false);
// The Snipping Tool engine is multilingual: the language only matters for Windows OCR.
const windowsEngine = computed(() => o.value.engine === 'windows' || !snippingInstalled.value);
const engines = computed(() =>
  (['snipping', 'windows'] as const).map((v) => ({ value: v, label: t(`settings.ocr.engines.${v}`) })),
);

const options = computed(() => [
  { value: 'auto', label: t('settings.ocr.auto') },
  ...installed.value.map((tag) => ({ value: tag, label: `${languageName(tag, locale.value)} (${tag})` })),
]);
</script>

<template>
  <div>
    <div
      v-if="windowsEngine && !installed.length"
      class="mb-4 rounded-md border border-warning/40 bg-warning/10 px-3 py-2 text-[12.5px] text-warning"
    >
      {{ t('settings.ocr.none') }}
    </div>
    <Card>
      <Field
        :label="t('settings.ocr.engine')"
        :description="snippingInstalled ? t('settings.ocr.engineDesc') : t('settings.ocr.snippingMissing')"
      >
        <Segmented v-if="snippingInstalled" v-model="o.engine" :options="engines" />
      </Field>
      <Field v-if="windowsEngine" :label="t('settings.ocr.language')">
        <Select v-model="o.language" :options="options" />
      </Field>
      <Field :label="t('settings.ocr.translate')" :description="t('settings.ocr.translateDesc')">
        <Switch v-model="o.translate" />
      </Field>
    </Card>
  </div>
</template>
