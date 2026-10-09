<script setup lang="ts">
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { useSettings } from '@/lib/settings';
import Card from '@/ui/Card.vue';
import Field from '@/ui/Field.vue';
import Select from '@/ui/Select.vue';
import Switch from '@/ui/Switch.vue';

const { t } = useI18n();
const { settings } = useSettings();
const p = computed(() => settings.value.popup);

const placements = computed(() =>
  (['selection', 'cursor', 'last_position', 'screen_center'] as const).map((v) => ({
    value: v,
    label: t(`settings.popup.placements.${v}`),
  })),
);
const widths = computed(() => {
  const opts = [520, 600, 720, 860].map((w) => ({ value: w, label: `${w}px` }));
  if (!opts.some((o) => o.value === p.value.width)) opts.unshift({ value: p.value.width, label: `${p.value.width}px` });
  return opts;
});
</script>

<template>
  <Card>
    <Field :label="t('settings.popup.placement')">
      <Select v-model="p.placement" :options="placements" />
    </Field>
    <Field :label="t('settings.popup.closeOnBlur')">
      <Switch v-model="p.close_on_blur" />
    </Field>
    <Field :label="t('settings.popup.alwaysPinned')">
      <Switch v-model="p.always_pinned" />
    </Field>
    <Field :label="t('settings.popup.size')">
      <Select v-model="p.width" :options="widths" />
    </Field>
  </Card>
</template>
