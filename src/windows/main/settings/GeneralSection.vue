<script setup lang="ts">
import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { commands } from '@/lib/api';
import { useSettings } from '@/lib/settings';
import { languageName } from '@/lib/langs';
import { UI_LANGUAGES } from '@/i18n';
import Button from '@/ui/Button.vue';
import Card from '@/ui/Card.vue';
import Field from '@/ui/Field.vue';
import Segmented from '@/ui/Segmented.vue';
import Select from '@/ui/Select.vue';
import Switch from '@/ui/Switch.vue';
import UpdatesCard from './UpdatesCard.vue';

const { t } = useI18n();
const { settings, appInfo } = useSettings();
const g = computed(() => settings.value.general);

// "Paused" in settings; shown positively as "automatic translation is on".
const autoTriggers = computed({
  get: () => !settings.value.paused,
  set: (on: boolean) => {
    commands.setPaused(!on);
  },
});

const uiLanguages = computed(() => [
  { value: 'system', label: t('settings.general.system') },
  ...UI_LANGUAGES.map((l) => ({ value: l, label: languageName(l, l) })),
]);
const themes = computed(() =>
  (['system', 'light', 'dark'] as const).map((v) => ({ value: v, label: t(`settings.general.themes.${v}`) })),
);
const effects = computed(() =>
  (['auto', 'mica', 'acrylic', 'none'] as const).map((v) => ({ value: v, label: t(`settings.general.effects.${v}`) })),
);
const sizes = [14, 15, 16, 17, 18, 20, 22, 24].map((n) => ({ value: n, label: `${n}px` }));
const scales = [80, 90, 100, 110, 125, 150].map((n) => ({ value: n, label: `${n}%` }));
</script>

<template>
  <div>
    <Card>
      <Field :label="t('settings.general.autoTriggers')" :description="t('settings.general.autoTriggersDesc')">
        <Switch v-model="autoTriggers" />
      </Field>
    </Card>

    <Card>
      <Field :label="t('settings.general.uiLanguage')">
        <Select v-model="g.ui_language" :options="uiLanguages" />
      </Field>
      <Field :label="t('settings.general.theme')">
        <Segmented v-model="g.theme" :options="themes" />
      </Field>
      <Field :label="t('settings.general.effect')" :description="t('settings.general.effectDesc')">
        <Select v-model="g.window_effect" :options="effects" />
      </Field>
      <Field :label="t('settings.general.uiScale')" :description="t('settings.general.uiScaleDesc')">
        <Select v-model="g.ui_scale" :options="scales" />
      </Field>
      <Field :label="t('settings.general.fontSize')">
        <Select v-model="g.font_size" :options="sizes" />
      </Field>
    </Card>

    <Card>
      <Field :label="t('settings.general.autostart')">
        <Switch v-model="g.autostart" />
      </Field>
      <Field :label="t('settings.general.showMain')">
        <Switch v-model="g.show_main_on_start" />
      </Field>
      <Field :label="t('settings.general.closeToTray')">
        <Switch v-model="g.close_to_tray" />
      </Field>
    </Card>

    <UpdatesCard />

    <div class="flex items-center gap-2 px-1">
      <Button variant="danger" size="sm" @click="commands.quit()">{{ t('settings.general.quit') }}</Button>
      <div class="flex-1" />
      <span class="text-[12px] text-faint">1lang {{ appInfo?.version }} · MIT</span>
    </div>
  </div>
</template>
