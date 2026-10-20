import { computed, reactive } from 'vue';
import { commands, type CheckLevel, type CheckResult, type Origin, type ProviderKind } from './api';
import { useSettings } from './settings';

const MACHINE: ProviderKind[] = ['deepl', 'google_cloud', 'google_free', 'microsoft'];
export const isLlm = (kind: ProviderKind) => !MACHINE.includes(kind);

/** Enabled AI providers: a writing check needs at least one. */
export function useLlmProviders() {
  const { settings } = useSettings();
  return computed(() => settings.value.providers.filter((p) => p.enabled && isLlm(p.kind)));
}

export interface CheckState {
  status: 'idle' | 'loading' | 'done' | 'error';
  result: CheckResult | null;
  error: string | null;
  /** Per segment: the edit is accepted (unchanged pieces are always true). */
  accepted: boolean[];
}

export interface CheckOptions {
  text: string;
  level: CheckLevel;
  context?: string | null;
  providerId?: string | null;
  origin: Origin;
  appExe?: string | null;
}

let counter = 0;

export function useChecker() {
  const state = reactive<CheckState>({ status: 'idle', result: null, error: null, accepted: [] });
  let currentId: string | null = null;

  function cancel() {
    if (currentId) {
      commands.cancelTranslation(currentId);
      currentId = null;
    }
  }

  function reset() {
    cancel();
    Object.assign(state, { status: 'idle', result: null, error: null, accepted: [] });
  }

  async function run(o: CheckOptions): Promise<CheckResult | null> {
    cancel();
    const id = `check-${Date.now()}-${++counter}`;
    currentId = id;
    Object.assign(state, { status: 'loading', error: null });
    const res = await commands.check(
      {
        text: o.text,
        level: o.level,
        explain_lang: null,
        context: o.context ?? null,
        provider_id: o.providerId ?? null,
      },
      id,
      o.origin,
      o.appExe ?? null,
      true,
    );
    if (currentId !== id) return null;
    currentId = null;
    if (res.status === 'error') {
      if (res.error !== 'cancelled') Object.assign(state, { status: 'error', error: res.error });
      return null;
    }
    Object.assign(state, {
      status: 'done',
      result: res.data,
      accepted: res.data.segments.map(() => true),
    });
    return res.data;
  }

  /** The text with the accepted edits applied. */
  const finalText = computed(() =>
    (state.result?.segments ?? []).map((s, i) => (state.accepted[i] ? s.corrected : s.original)).join(''),
  );

  const hasEdits = computed(() => (state.result?.segments ?? []).some((s) => s.original !== s.corrected));

  return { state, run, cancel, reset, finalText, hasEdits };
}
