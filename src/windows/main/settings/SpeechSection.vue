<script setup lang="ts">
import { computed, onMounted, ref, shallowRef } from 'vue';
import { Channel } from '@tauri-apps/api/core';
import { Square, Volume2 } from '@lucide/vue';
import { useI18n } from 'vue-i18n';
import { commands, unwrap, type Engine, type InstallProgress, type KokoroStatus, type Voice } from '@/lib/api';
import { useSettings } from '@/lib/settings';
import { languageName } from '@/lib/langs';
import { previewLanguage, previewVoice, SAMPLES, useSpeech } from '@/lib/speech';
import Button from '@/ui/Button.vue';
import Card from '@/ui/Card.vue';
import Field from '@/ui/Field.vue';
import Segmented from '@/ui/Segmented.vue';
import Select from '@/ui/Select.vue';
import Spinner from '@/ui/Spinner.vue';
import Switch from '@/ui/Switch.vue';

const { t, locale } = useI18n();
const { settings } = useSettings();
const sp = computed(() => settings.value.speech);
const speech = useSpeech();

const engines = computed(() =>
  (['edge', 'kokoro'] as const).map((v) => ({ value: v, label: t(`settings.speech.engines.${v}`) })),
);
const rates = [60, 70, 80, 90, 100, 110, 120, 130, 150].map((n) => ({ value: n, label: `${n}%` }));
const slowRates = [50, 60, 70, 80, 90].map((n) => ({ value: n, label: `${n}%` }));

// ---------- offline voice ----------

const kokoro = ref<KokoroStatus | null>(null);
const progress = ref<InstallProgress | null>(null);
const installError = ref<string | null>(null);

const percent = computed(() => {
  const p = progress.value;
  return p ? Math.floor(((p.done ?? 0) / (p.total || 1)) * 100) : 0;
});
const sizeText = computed(() =>
  new Intl.NumberFormat(locale.value, { style: 'unit', unit: 'megabyte', maximumFractionDigits: 0 }).format(
    (kokoro.value?.download_size ?? 0) / 1e6,
  ),
);
const kokoroLangs = computed(() => (kokoro.value?.languages ?? []).map((l) => languageName(l, locale.value)).join(', '));
const kokoroReady = computed(() => !!kokoro.value?.installed);

const kokoroText = computed(() => {
  if (installError.value) return installError.value;
  if (kokoro.value?.installing || progress.value) return t('settings.speech.downloading', { percent: percent.value });
  if (kokoroReady.value) return t('settings.speech.installed', { langs: kokoroLangs.value });
  return t('settings.speech.notInstalled', { size: sizeText.value });
});

async function install() {
  installError.value = null;
  progress.value = { stage: 'model', done: 0, total: kokoro.value?.download_size ?? 1 };
  const channel = new Channel<InstallProgress>();
  channel.onmessage = (p) => (progress.value = p);
  try {
    await unwrap(commands.kokoroInstall(channel));
  } catch (e) {
    installError.value = e instanceof Error ? e.message : String(e);
  }
  progress.value = null;
  kokoro.value = await commands.kokoroStatus();
}

async function remove() {
  await commands.kokoroRemove();
  kokoro.value = await commands.kokoroStatus();
}

// ---------- voices ----------

const edgeVoices = shallowRef<Voice[]>([]);
const kokoroVoices = shallowRef<Voice[]>([]);
const voicesError = ref<string | null>(null);

onMounted(async () => {
  kokoro.value = await commands.kokoroStatus();
  kokoroVoices.value = await unwrap(commands.speechVoices('kokoro')).catch(() => []);
  try {
    edgeVoices.value = await unwrap(commands.speechVoices('edge'));
  } catch (e) {
    voicesError.value = t('settings.speech.voicesError', { error: e instanceof Error ? e.message : String(e) });
  }
});

const langs = computed(() => settings.value.languages.favorites);

/** Engine that speaks a language with the current settings. */
function engineFor(lang: string): Engine {
  return sp.value.engine === 'kokoro' && kokoroReady.value && kokoro.value?.languages.includes(lang) ? 'kokoro' : 'edge';
}

function voiceMap(lang: string) {
  return engineFor(lang) === 'kokoro' ? sp.value.kokoro_voices : sp.value.edge_voices;
}

function regionName(localeTag: string) {
  const region = localeTag.split('-')[1];
  if (!region || region.length !== 2) return '';
  try {
    return new Intl.DisplayNames([locale.value], { type: 'region' }).of(region) ?? region;
  } catch {
    return region;
  }
}

function voiceOptions(lang: string) {
  const list = engineFor(lang) === 'kokoro' ? kokoroVoices.value : edgeVoices.value;
  return [
    { value: 'auto', label: t('settings.speech.auto') },
    ...list
      .filter((v) => v.locale.split('-')[0] === lang)
      .map((v) => ({ value: v.id, label: [v.name, regionName(v.locale)].filter(Boolean).join(' · ') })),
  ];
}

function getVoice(lang: string) {
  return voiceMap(lang)[lang] ?? 'auto';
}

function setVoice(lang: string, voice: string) {
  const map = voiceMap(lang);
  if (voice === 'auto') delete map[lang];
  else map[lang] = voice;
}

function voiceNote(lang: string) {
  if (sp.value.engine === 'kokoro' && kokoroReady.value && engineFor(lang) === 'edge') {
    return t('settings.speech.onlineFallback');
  }
  return undefined;
}

function preview(lang: string) {
  const text = SAMPLES[lang] ?? SAMPLES.en;
  const voice = getVoice(lang);
  const owner = `preview:${lang}`;
  if (voice === 'auto') previewLanguage(text, lang, owner);
  else previewVoice(text, engineFor(lang), voice, owner);
}

function previewState(lang: string) {
  return speech.owner === `preview:${lang}` ? speech.status : 'idle';
}
</script>

<template>
  <div>
    <Card>
      <Field
        :label="t('settings.speech.engine')"
        :description="sp.engine === 'kokoro' ? t('settings.speech.kokoroDesc') : t('settings.speech.edgeDesc')"
      >
        <Segmented v-model="sp.engine" :options="engines" />
      </Field>
      <Field :label="t('settings.speech.offline')" :description="kokoroText">
        <template v-if="progress || kokoro?.installing">
          <div class="h-2 w-40 overflow-hidden rounded-full bg-nav-selected">
            <div class="bg-accent h-full rounded-full transition-[width] duration-300" :style="{ width: `${percent}%` }" />
          </div>
        </template>
        <Button v-else-if="kokoroReady" variant="danger" size="sm" @click="remove">
          {{ t('settings.speech.remove') }}
        </Button>
        <Button v-else :variant="sp.engine === 'kokoro' ? 'primary' : 'secondary'" size="sm" @click="install">
          {{ t('settings.speech.download') }}
        </Button>
      </Field>
    </Card>
    <div
      v-if="sp.engine === 'kokoro' && kokoro && !kokoroReady"
      class="-mt-4 mb-7 rounded-2xl border border-warning/40 bg-warning/10 px-4 py-3 text-[13.5px] text-warning"
    >
      {{ t('settings.speech.needsDownload') }}
    </div>

    <Card :title="t('settings.speech.voices')" :description="voicesError ?? t('settings.speech.voicesDesc')">
      <Field v-for="code in langs" :key="code" :label="languageName(code, locale)" :description="voiceNote(code)">
        <Select
          :model-value="getVoice(code)"
          :options="voiceOptions(code)"
          trigger-class="w-60"
          @update:model-value="(v) => setVoice(code, v)"
        />
        <Button variant="ghost" size="icon" :title="t('speech.listen')" @click="preview(code)">
          <Spinner v-if="previewState(code) === 'loading'" class="size-5" />
          <Square v-else-if="previewState(code) === 'playing'" class="size-5 fill-current" />
          <Volume2 v-else class="size-5" />
        </Button>
      </Field>
    </Card>

    <Card>
      <Field :label="t('settings.speech.speed')">
        <Select v-model="sp.rate" :options="rates" />
      </Field>
      <Field :label="t('settings.speech.slowSpeed')" :description="t('settings.speech.slowSpeedDesc')">
        <Select v-model="sp.slow_rate" :options="slowRates" />
      </Field>
    </Card>

    <Card>
      <Field :label="t('settings.speech.recordings')" :description="t('settings.speech.recordingsDesc')">
        <Switch v-model="sp.word_recordings" />
      </Field>
    </Card>
  </div>
</template>
