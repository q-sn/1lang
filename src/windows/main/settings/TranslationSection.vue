<script setup lang="ts">
import { computed, ref } from 'vue';
import { useI18n } from 'vue-i18n';
import { commands } from '@/lib/api';
import { useSettings } from '@/lib/settings';
import Button from '@/ui/Button.vue';
import Card from '@/ui/Card.vue';
import Field from '@/ui/Field.vue';
import Select from '@/ui/Select.vue';
import Switch from '@/ui/Switch.vue';

const { t } = useI18n();
const { settings } = useSettings();
const tr = computed(() => settings.value.translation);
const cleared = ref(false);

const formality = computed(() =>
  (['default', 'formal', 'informal'] as const).map((v) => ({ value: v, label: t(`translate.formality.${v}`) })),
);
const limits = [500, 1000, 5000, 20000, 100000].map((n) => ({ value: n, label: n.toLocaleString() }));

async function clearCache() {
  await commands.cacheClear();
  cleared.value = true;
  setTimeout(() => (cleared.value = false), 1500);
}
</script>

<template>
  <div>
    <Card>
      <Field :label="t('settings.translation.formality')">
        <Select v-model="tr.formality" :options="formality" />
      </Field>
      <Field :label="t('settings.translation.dictionary')" :description="t('settings.translation.dictionaryDesc')">
        <Switch v-model="tr.dictionary_for_words" />
      </Field>
      <Field :label="t('settings.translation.context')" :description="t('settings.translation.contextDesc')">
        <Switch v-model="tr.use_context" />
      </Field>
      <Field :label="t('settings.translation.custom')" :description="t('settings.translation.customDesc')" stacked>
        <textarea
          v-model.lazy="tr.custom_instructions"
          rows="3"
          class="w-full resize-y rounded-md border border-line bg-surface-strong px-2.5 py-2 text-[13px] outline-none focus:border-line-strong"
        />
      </Field>
    </Card>

    <Card>
      <Field :label="t('settings.translation.cache')" :description="t('settings.translation.cacheDesc')">
        <Button size="sm" variant="ghost" @click="clearCache">
          {{ cleared ? t('settings.saved') : t('settings.translation.clearCache') }}
        </Button>
        <Switch v-model="tr.cache" />
      </Field>
      <Field :label="t('settings.translation.history')">
        <Switch v-model="tr.history" />
      </Field>
      <Field :label="t('settings.translation.historyLimit')">
        <Select v-model="tr.history_limit" :options="limits" />
        <span class="text-[12.5px] text-muted">{{ t('settings.translation.entries') }}</span>
      </Field>
    </Card>
  </div>
</template>
