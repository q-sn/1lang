<script setup lang="ts">
import { useI18n } from 'vue-i18n';
import { useSettings } from '@/lib/settings';
import Card from '@/ui/Card.vue';
import Field from '@/ui/Field.vue';
import HotkeyInput from '@/ui/HotkeyInput.vue';

const { t } = useI18n();
const { settings } = useSettings();
const keys = ['translate_selection', 'replace_selection', 'ocr_region', 'translate_clipboard', 'open_main'] as const;
</script>

<template>
  <Card>
    <Field v-for="k in keys" :key="k" :label="t(`settings.hotkeys.${k}`)">
      <HotkeyInput v-model="settings.hotkeys[k]" />
    </Field>
  </Card>
</template>
