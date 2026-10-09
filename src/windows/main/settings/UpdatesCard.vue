<script setup lang="ts">
import { computed, onMounted, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { commands, events, type UpdateStatus } from '@/lib/api';
import { useSettings } from '@/lib/settings';
import Button from '@/ui/Button.vue';
import Card from '@/ui/Card.vue';
import Field from '@/ui/Field.vue';
import Spinner from '@/ui/Spinner.vue';
import Switch from '@/ui/Switch.vue';

const { t, locale } = useI18n();
const { settings } = useSettings();
const status = ref<UpdateStatus | null>(null);

onMounted(async () => {
  status.value = await commands.updateStatus();
  await events.updateStatus.listen((e) => (status.value = { ...e.payload, current: status.value?.current ?? '' }));
});

const busy = computed(() => status.value?.phase === 'checking' || status.value?.phase === 'downloading');

const text = computed(() => {
  const s = status.value;
  if (!s) return '';
  switch (s.phase) {
    case 'checking':
      return t('settings.updates.checking');
    case 'downloading':
      return t('settings.updates.downloading', { version: s.latest });
    case 'ready':
      return t('settings.updates.ready', { version: s.latest });
    case 'up_to_date':
      return t('settings.updates.upToDate');
    case 'error':
      return t('settings.updates.error', { error: s.error });
    default:
      return s.checked_at ? '' : t('settings.updates.never');
  }
});

const checkedAt = computed(() => {
  const at = status.value?.checked_at;
  if (!at) return '';
  return new Intl.DateTimeFormat(locale.value, { dateStyle: 'medium', timeStyle: 'short' }).format(new Date(at));
});

async function check() {
  status.value = await commands.checkUpdates();
}
</script>

<template>
  <Card :title="t('settings.updates.title')">
    <Field :label="t('settings.updates.auto')" :description="t('settings.updates.autoDesc')">
      <Switch v-model="settings.general.auto_update" />
    </Field>
    <Field
      :label="t('settings.updates.version', { version: status?.current ?? '' })"
      :description="[text, checkedAt && t('settings.updates.checkedAt', { date: checkedAt })].filter(Boolean).join(' · ')"
    >
      <Button v-if="status?.phase === 'ready'" variant="primary" @click="commands.restartToUpdate()">
        {{ t('settings.updates.restart') }}
      </Button>
      <Button v-else :disabled="busy" @click="check">
        <Spinner v-if="busy" class="size-4" />
        {{ t('settings.updates.check') }}
      </Button>
    </Field>
  </Card>
</template>
