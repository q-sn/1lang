<script setup lang="ts">
import { computed, ref } from 'vue';
import { ArrowRight, Check, CircleCheck, Undo2 } from '@lucide/vue';
import { useI18n } from 'vue-i18n';
import type { CheckIssue, CheckResult } from '@/lib/api';
import Button from './Button.vue';
import Tip from './Tip.vue';

/** Corrected text with edits that can be accepted one by one, and the explanations. */
const props = defineProps<{ result: CheckResult; showEdits: boolean }>();
const accepted = defineModel<boolean[]>('accepted', { required: true });
const { t } = useI18n();

const hoveredIssue = ref<number | null>(null);
const hoveredSegment = ref<number | null>(null);

/** Spaces around an edit stay outside the strike-through and the highlight. */
function ws(s: string) {
  const core = s.trim();
  if (!core) return { lead: '', core: s, trail: '' };
  const start = s.indexOf(core);
  return { lead: s.slice(0, start), core, trail: s.slice(start + core.length) };
}

const isEdit = (i: number) => props.result.segments[i].original !== props.result.segments[i].corrected;

/** Issues that explain each segment. */
const issuesOf = computed(() => {
  const map = props.result.segments.map(() => [] as number[]);
  props.result.issues.forEach((issue, k) => (issue.segments ?? []).forEach((s) => map[s]?.push(k)));
  return map;
});

/** An edit explained only by suggestions is not a mistake. */
const isSuggestion = (i: number) => {
  const linked = issuesOf.value[i];
  return linked.length > 0 && linked.every((k) => props.result.issues[k].severity === 'suggestion');
};

const highlighted = (i: number) =>
  hoveredSegment.value === i ||
  (hoveredIssue.value !== null && (props.result.issues[hoveredIssue.value].segments ?? []).includes(i));

function toggleSegment(i: number) {
  accepted.value = accepted.value.map((a, j) => (j === i ? !a : a));
}

const issueAccepted = (issue: CheckIssue) => (issue.segments ?? []).every((s) => accepted.value[s]);

function toggleIssue(issue: CheckIssue) {
  const on = !issueAccepted(issue);
  const segs = new Set(issue.segments ?? []);
  accepted.value = accepted.value.map((a, j) => (segs.has(j) ? on : a));
}

const counts = computed(() => {
  const errors = props.result.issues.filter((i) => i.severity === 'error').length;
  return { errors, suggestions: props.result.issues.length - errors };
});
const clean = computed(() => !props.result.issues.length && !props.result.segments.some((_, i) => isEdit(i)));
</script>

<template>
  <div class="selectable space-y-4 text-[var(--text-size)]">
    <div v-if="clean" class="flex items-start gap-3 rounded-2xl bg-success/10 px-4 py-3.5">
      <CircleCheck class="mt-0.5 size-6 shrink-0 text-success" />
      <div>
        <div class="font-semibold text-success">{{ t('check.noIssues') }}</div>
        <div class="text-[0.88em] text-muted">{{ t('check.noIssuesDesc') }}</div>
      </div>
    </div>

    <!-- corrected text -->
    <div v-else class="result-text">
      <template v-for="(seg, i) in result.segments" :key="i">
        <span v-if="!isEdit(i)">{{ seg.original }}</span>
        <span
          v-else-if="!accepted[i]"
          :class="[
            'cursor-pointer rounded-[4px] underline decoration-faint decoration-dashed underline-offset-4 transition-colors',
            highlighted(i) && 'bg-hover',
          ]"
          :title="t('check.acceptEdit')"
          @mouseenter="hoveredSegment = i"
          @mouseleave="hoveredSegment = null"
          @click="toggleSegment(i)"
        >{{ seg.original || '⁁' }}</span>
        <span
          v-else
          :class="['cursor-pointer rounded-[5px] transition-colors', highlighted(i) && 'bg-hover']"
          :title="t('check.rejectEdit')"
          @mouseenter="hoveredSegment = i"
          @mouseleave="hoveredSegment = null"
          @click="toggleSegment(i)"
        >
          <template v-if="showEdits && seg.original.trim()">{{ ws(seg.original).lead }}<del
            :class="['text-muted decoration-danger/70 decoration-[1.5px]', seg.corrected && 'mr-1']"
          >{{ ws(seg.original).core }}</del>{{ seg.corrected ? '' : ws(seg.original).trail }}</template>
          <template v-if="seg.corrected">{{ ws(seg.corrected).lead }}<ins
            :class="[
              'rounded-[4px] px-0.5 no-underline',
              showEdits
                ? isSuggestion(i) ? 'bg-accent/15 text-accent' : 'bg-success/15 text-success'
                : 'underline decoration-2 underline-offset-4 ' + (isSuggestion(i) ? 'decoration-accent/50' : 'decoration-success/50'),
            ]"
          >{{ ws(seg.corrected).core }}</ins>{{ ws(seg.corrected).trail }}</template>
        </span>
      </template>
    </div>

    <!-- explanations -->
    <template v-if="result.issues.length">
      <div class="flex items-center gap-2 border-t border-line pt-3 text-[13px] font-medium text-muted">
        <span v-if="counts.errors" class="flex items-center gap-1.5">
          <span class="size-2 rounded-full bg-danger" />{{ t('check.errors', counts.errors) }}
        </span>
        <span v-if="counts.errors && counts.suggestions" class="text-faint">·</span>
        <span v-if="counts.suggestions" class="flex items-center gap-1.5">
          <span class="size-2 rounded-full bg-accent" />{{ t('check.suggestions', counts.suggestions) }}
        </span>
      </div>
      <ol class="space-y-2">
        <li
          v-for="(issue, k) in result.issues"
          :key="k"
          :class="[
            'rounded-2xl border px-4 py-3 transition-[background-color,border-color,opacity] duration-150',
            hoveredIssue === k || (hoveredSegment !== null && issuesOf[hoveredSegment]?.includes(k))
              ? 'border-line-strong bg-hover'
              : 'border-line',
            !issueAccepted(issue) && 'opacity-55',
          ]"
          @mouseenter="hoveredIssue = k"
          @mouseleave="hoveredIssue = null"
        >
          <div class="flex items-center gap-2">
            <span
              :class="[
                'rounded-md px-1.5 py-0.5 text-[12px] font-semibold',
                issue.severity === 'error' ? 'bg-danger/10 text-danger' : 'bg-accent/10 text-accent',
              ]"
            >{{ t(`check.kind.${issue.kind}`) }}</span>
            <div class="min-w-0 flex-1" />
            <Tip v-if="issue.segments?.length" :text="issueAccepted(issue) ? t('check.reject') : t('check.accept')">
              <Button variant="ghost" size="icon-sm" @click="toggleIssue(issue)">
                <Undo2 v-if="issueAccepted(issue)" class="size-4" />
                <Check v-else class="size-4" />
              </Button>
            </Tip>
          </div>
          <div v-if="issue.original || issue.correction" class="mt-1 flex flex-wrap items-center gap-x-2 gap-y-0.5 text-[0.92em]">
            <span v-if="issue.original" class="text-muted line-through decoration-danger/60">{{ issue.original }}</span>
            <ArrowRight v-if="issue.original && issue.correction" class="size-3.5 shrink-0 text-faint" />
            <span class="font-semibold">{{ issue.correction }}</span>
          </div>
          <p class="mt-1 text-[0.86em] leading-relaxed text-muted">{{ issue.explanation }}</p>
        </li>
      </ol>
    </template>
  </div>
</template>
