<script setup lang="ts">
import { computed } from 'vue';
import { CircleAlert, Square, Turtle, Volume2 } from '@lucide/vue';
import { useI18n } from 'vue-i18n';
import { speak, useSpeech } from '@/lib/speech';
import Button from './Button.vue';
import Spinner from './Spinner.vue';
import Tip from './Tip.vue';

const props = withDefaults(
  defineProps<{
    text: string;
    lang: string | null | undefined;
    /** Unique per button in the window. */
    owner: string;
    small?: boolean;
    /** Show the "slowly" button next to it. */
    slow?: boolean;
    disabled?: boolean;
  }>(),
  { slow: true },
);

const { t } = useI18n();
const speech = useSpeech();

function stateOf(owner: string) {
  if (speech.error?.owner === owner) return 'error';
  return speech.owner === owner ? speech.status : 'idle';
}
const normal = computed(() => stateOf(props.owner));
const slowState = computed(() => stateOf(`${props.owner}:slow`));

function tip(s: string, label: string) {
  if (s === 'error') return speech.error!.message;
  return s === 'playing' ? t('speech.stop') : label;
}

const size = computed(() => (props.small ? 'icon-sm' : 'icon'));
const iconClass = computed(() => (props.small ? 'size-[18px]' : 'size-5'));
</script>

<template>
  <div class="inline-flex shrink-0 items-center gap-0.5">
    <Tip :text="tip(normal, t('speech.listen'))">
      <Button variant="ghost" :size="size" :disabled="disabled || !text" @click="speak(text, lang, { owner })">
        <Spinner v-if="normal === 'loading'" :class="iconClass" />
        <Square v-else-if="normal === 'playing'" :class="[iconClass, 'fill-current']" />
        <CircleAlert v-else-if="normal === 'error'" :class="[iconClass, 'text-danger']" />
        <Volume2 v-else :class="iconClass" />
      </Button>
    </Tip>
    <Tip v-if="slow" :text="tip(slowState, t('speech.slow'))">
      <Button variant="ghost" :size="size" :disabled="disabled || !text" @click="speak(text, lang, { owner, slow: true })">
        <Spinner v-if="slowState === 'loading'" :class="iconClass" />
        <Square v-else-if="slowState === 'playing'" :class="[iconClass, 'fill-current']" />
        <CircleAlert v-else-if="slowState === 'error'" :class="[iconClass, 'text-danger']" />
        <Turtle v-else :class="iconClass" />
      </Button>
    </Tip>
  </div>
</template>
