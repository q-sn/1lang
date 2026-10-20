<script setup lang="ts">
import { Check } from '@lucide/vue';
import { useI18n } from 'vue-i18n';
import { useLlmProviders } from '@/lib/checker';
import { navigate } from '@/lib/nav';
import { useSettings } from '@/lib/settings';
import Button from '@/ui/Button.vue';
import Card from '@/ui/Card.vue';
import Field from '@/ui/Field.vue';
import HotkeyInput from '@/ui/HotkeyInput.vue';

const { t } = useI18n();
const { settings } = useSettings();
const llms = useLlmProviders();
const levels = ['errors', 'natural', 'rewrite'] as const;
</script>

<template>
  <div>
    <Card :title="t('settings.check.defaultLevel')" :description="t('settings.check.defaultLevelDesc')">
      <button
        v-for="l in levels"
        :key="l"
        :class="[
          'flex w-full items-center gap-4 px-5 py-3.5 text-left transition-colors [&+&]:border-t [&+&]:border-line',
          settings.check.default_level === l ? 'bg-nav-selected' : 'hover:bg-hover',
        ]"
        @click="settings.check.default_level = l"
      >
        <div class="min-w-0 flex-1">
          <div class="text-[14.5px] font-medium">{{ t(`check.level.${l}`) }}</div>
          <div class="mt-0.5 text-[13px] leading-snug text-muted">{{ t(`check.levelDesc.${l}`) }}</div>
        </div>
        <Check v-if="settings.check.default_level === l" class="size-5 shrink-0 text-accent" />
      </button>
    </Card>

    <Card>
      <Field :label="t('settings.hotkeys.check_selection')" :description="t('settings.check.hotkeyDesc')">
        <HotkeyInput v-model="settings.hotkeys.check_selection" />
      </Field>
      <Field
        :label="t('settings.check.providers')"
        :description="llms.length ? llms.map((p) => p.name).join(', ') : t('settings.check.noProviders')"
      >
        <Button size="sm" :variant="llms.length ? 'ghost' : 'primary'" @click="navigate('settings/providers')">
          {{ llms.length ? t('check.openProviders') : t('check.connectAi') }}
        </Button>
      </Field>
    </Card>
  </div>
</template>
