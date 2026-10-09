import { reactive } from 'vue';
import { Channel } from '@tauri-apps/api/core';
import {
  commands,
  type DictionaryEntry,
  type Origin,
  type OutputKind,
  type TranslateEvent,
  type TranslateMode,
  type TranslateRequest,
  type TranslationResult,
} from './api';
import { useSettings } from './settings';

export type Status = 'idle' | 'loading' | 'streaming' | 'done' | 'error';

export interface TranslatorState {
  status: Status;
  text: string;
  error: string | null;
  result: TranslationResult | null;
  dictionary: DictionaryEntry | null;
  kind: OutputKind;
  providerName: string | null;
  model: string | null;
  sourceLang: string | null;
  targetLang: string | null;
}

export interface RunOptions {
  text: string;
  sourceLang?: string | null;
  targetLang?: string | null;
  mode?: TranslateMode;
  context?: string | null;
  providerId?: string | null;
  origin: Origin;
  appExe?: string | null;
  saveHistory: boolean;
}

let counter = 0;

/** Streaming translation state for one output area. */
export function useTranslator() {
  const { settings } = useSettings();
  const state = reactive<TranslatorState>({
    status: 'idle',
    text: '',
    error: null,
    result: null,
    dictionary: null,
    kind: 'text',
    providerName: null,
    model: null,
    sourceLang: null,
    targetLang: null,
  });
  /** Request that can still be cancelled. */
  let currentId: string | null = null;
  /** Latest request: its events are applied even after the command returned,
   *  because channel messages may arrive after the command's reply. */
  let activeId: string | null = null;

  function finish(r: TranslationResult) {
    state.result = r;
    state.text = r.text;
    state.dictionary = r.dictionary;
    state.kind = r.kind;
    state.sourceLang = r.source_lang;
    state.targetLang = r.target_lang;
    state.providerName = r.provider_name;
    state.model = r.model;
    state.error = null;
    state.status = 'done';
  }

  function reset() {
    cancel();
    activeId = null;
    Object.assign(state, {
      status: 'idle', text: '', error: null, result: null, dictionary: null, kind: 'text',
      providerName: null, model: null, sourceLang: null, targetLang: null,
    });
  }

  function cancel() {
    if (currentId) {
      commands.cancelTranslation(currentId);
      currentId = null;
    }
  }

  async function run(o: RunOptions): Promise<TranslationResult | null> {
    cancel();
    const id = `${Date.now()}-${++counter}`;
    currentId = id;
    activeId = id;
    Object.assign(state, {
      status: 'loading', text: '', error: null, result: null, dictionary: null, kind: 'text',
      sourceLang: o.sourceLang ?? null, targetLang: o.targetLang ?? null,
    });

    const channel = new Channel<TranslateEvent>();
    channel.onmessage = (ev) => {
      if (activeId !== id || state.status === 'done') return;
      switch (ev.type) {
        case 'started':
          state.providerName = ev.provider_name;
          state.model = ev.model;
          state.kind = ev.kind;
          state.sourceLang = ev.source_lang ?? state.sourceLang;
          state.targetLang = ev.target_lang ?? state.targetLang;
          state.text = '';
          break;
        case 'direction':
          state.sourceLang = ev.source_lang ?? state.sourceLang;
          state.targetLang = ev.target_lang;
          break;
        case 'delta':
          state.status = 'streaming';
          state.text += ev.text;
          break;
        case 'reset':
          state.text = '';
          state.status = 'loading';
          break;
        case 'done':
          finish(ev.result);
          break;
        case 'error':
          state.error = ev.message;
          state.status = 'error';
          break;
      }
    };

    const request: TranslateRequest = {
      text: o.text,
      source_lang: o.sourceLang ?? null,
      target_lang: o.targetLang ?? null,
      mode: o.mode ?? 'auto',
      context: o.context ?? null,
      formality: settings.value.translation.formality,
      provider_id: o.providerId ?? null,
    };
    const res = await commands.translate(request, id, o.origin, o.appExe ?? null, o.saveHistory, channel);
    if (activeId !== id) return null;
    if (currentId === id) currentId = null;
    if (res.status === 'error') {
      if (res.error !== 'cancelled' && state.status !== 'error') {
        state.error = res.error;
        state.status = 'error';
      }
      return null;
    }
    // The command result is authoritative even if the `done` event is still in flight.
    finish(res.data);
    return res.data;
  }

  return { state, run, cancel, reset };
}

/** Speak text with the system voices (Web Speech API in WebView2). */
export function speak(text: string, lang: string | null) {
  const synth = window.speechSynthesis;
  if (!synth) return;
  if (synth.speaking) {
    synth.cancel();
    return;
  }
  const u = new SpeechSynthesisUtterance(text);
  if (lang) {
    u.lang = lang;
    const voice = synth.getVoices().find((v) => v.lang.toLowerCase().startsWith(lang.toLowerCase()));
    if (voice) u.voice = voice;
  }
  synth.speak(u);
}

export function formatCost(usd: number | null | undefined): string {
  if (usd == null) return '';
  if (usd === 0) return '$0';
  if (usd < 0.0001) return '<$0.0001';
  if (usd < 0.01) return `$${usd.toFixed(4)}`;
  return `$${usd.toFixed(2)}`;
}
