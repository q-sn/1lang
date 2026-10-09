<script setup lang="ts">
import { computed, ref } from 'vue';
import { X } from '@lucide/vue';
import { useI18n } from 'vue-i18n';
import { useSettings } from '@/lib/settings';
import Button from '@/ui/Button.vue';
import Card from '@/ui/Card.vue';
import Field from '@/ui/Field.vue';
import Segmented from '@/ui/Segmented.vue';
import Select from '@/ui/Select.vue';
import Switch from '@/ui/Switch.vue';
import TextInput from '@/ui/TextInput.vue';

const { t } = useI18n();
const { settings } = useSettings();
const s = computed(() => settings.value.selection);
const newApp = ref('');

const methods = computed(() =>
  (['uia_then_clipboard', 'uia', 'clipboard'] as const).map((v) => ({
    value: v,
    label: t(`settings.selection.methods.${v}`),
  })),
);
const anchors = computed(() =>
  (['selection', 'cursor'] as const).map((v) => ({ value: v, label: t(`settings.selection.anchors.${v}`) })),
);
const timeouts = computed(() => [
  ...[2000, 3000, 4000, 6000, 10000].map((ms) => ({ value: ms, label: t('settings.selection.seconds', { n: ms / 1000 }) })),
  { value: 0, label: t('settings.selection.never') },
]);
const detections = computed(() =>
  (['clipboard', 'keyboard'] as const).map((v) => ({ value: v, label: t(`settings.selection.detections.${v}`) })),
);
const intervals = [300, 400, 450, 600, 800].map((ms) => ({ value: ms, label: `${ms} ms` }));

function addApp() {
  const exe = newApp.value.trim().toLowerCase();
  if (!exe) return;
  const name = exe.endsWith('.exe') ? exe : `${exe}.exe`;
  if (!s.value.excluded_apps.includes(name)) s.value.excluded_apps.push(name);
  newApp.value = '';
}
</script>

<template>
  <div>
    <Card :title="t('settings.selection.reading')">
      <Field :label="t('settings.selection.method')" :description="t('settings.selection.methodDesc')">
        <Select v-model="s.method" :options="methods" />
      </Field>
      <Field :label="t('settings.selection.restoreClipboard')">
        <Switch v-model="s.restore_clipboard" />
      </Field>
    </Card>

    <Card :title="t('settings.selection.icon')">
      <Field :label="t('settings.selection.iconEnabled')">
        <Switch v-model="s.icon_enabled" />
      </Field>
      <template v-if="s.icon_enabled">
        <Field :label="t('settings.selection.iconAnchor')">
          <Segmented v-model="s.icon_anchor" :options="anchors" />
        </Field>
        <Field :label="t('settings.selection.iconTimeout')">
          <Select v-model="s.icon_timeout_ms" :options="timeouts" />
        </Field>
        <Field :label="t('settings.selection.iconClipboard')" :description="t('settings.selection.iconClipboardDesc')">
          <Switch v-model="s.icon_clipboard_fallback" />
        </Field>
      </template>
    </Card>

    <Card :title="t('settings.selection.doubleCopy')">
      <Field :label="t('settings.selection.doubleCopyEnabled')">
        <Switch v-model="s.double_copy_enabled" />
      </Field>
      <template v-if="s.double_copy_enabled">
        <Field :label="t('settings.selection.detection')" :description="t('settings.selection.detectionDesc')">
          <Segmented v-model="s.double_copy_detection" :options="detections" />
        </Field>
        <Field :label="t('settings.selection.interval')">
          <Select v-model="s.double_copy_interval_ms" :options="intervals" />
        </Field>
      </template>
    </Card>

    <Card :title="t('settings.selection.exclusions')" :description="t('settings.selection.exclusionsDesc')">
      <div class="flex flex-wrap gap-2 p-4">
        <span
          v-for="app in s.excluded_apps"
          :key="app"
          class="inline-flex h-9 items-center gap-1 rounded-full border border-line bg-surface-strong pl-4 pr-1.5 font-mono text-[13px]"
        >
          {{ app }}
          <button
            class="rounded-full p-0.5 text-muted hover:bg-hover hover:text-fg"
            @click="s.excluded_apps = s.excluded_apps.filter((a) => a !== app)"
          >
            <X class="size-3.5" />
          </button>
        </span>
      </div>
      <div class="flex gap-2 border-t border-line p-4">
        <TextInput v-model="newApp" mono :placeholder="t('settings.selection.exclusionPlaceholder')" @keydown.enter="addApp" />
        <Button @click="addApp">{{ t('common.add') }}</Button>
      </div>
    </Card>
  </div>
</template>
