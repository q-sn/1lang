import { reactive } from 'vue';
import { commands, unwrap, type ClipSource, type Engine, type SpeechClip } from './api';
import { useSettings } from './settings';

export interface SpeechState {
  /** Button that is loading or playing, e.g. "target" or "target:slow". */
  owner: string | null;
  status: 'idle' | 'loading' | 'playing';
  error: { owner: string; message: string } | null;
  /** Source of the last clip of each button (to show recordings). */
  sources: Record<string, ClipSource>;
}

/** One player per window: starting a clip stops the previous one. */
const state = reactive<SpeechState>({ owner: null, status: 'idle', error: null, sources: {} });
let audio: HTMLAudioElement | null = null;
let seq = 0;

export function useSpeech() {
  return state;
}

export function stopSpeech() {
  seq++;
  if (audio) {
    audio.pause();
    audio.removeAttribute('src');
    audio = null;
  }
  state.owner = null;
  state.status = 'idle';
}

/** Play a clip for `owner`; clicking the same button again stops it. */
async function play(owner: string, load: () => Promise<SpeechClip>) {
  if (state.owner === owner) {
    stopSpeech();
    return;
  }
  stopSpeech();
  const id = seq;
  state.owner = owner;
  state.status = 'loading';
  state.error = null;
  const done = () => {
    if (id !== seq) return;
    state.owner = null;
    state.status = 'idle';
    audio = null;
  };
  try {
    const clip = await load();
    if (id !== seq) return;
    state.sources[owner.replace(/:slow$/, '')] = clip.source;
    const a = new Audio(clip.url);
    // Recordings are slowed down here; keep the voice's pitch.
    a.preservesPitch = true;
    const rate = clip.playback_rate ?? 1;
    a.defaultPlaybackRate = rate;
    a.playbackRate = rate;
    a.onended = done;
    audio = a;
    await a.play();
    if (id === seq) state.status = 'playing';
  } catch (e) {
    if (id !== seq) return;
    state.error = { owner, message: e instanceof Error ? e.message : String(e) };
    done();
  }
}

export interface SpeakOptions {
  /** Identifies the button, for its loading / playing state. */
  owner: string;
  slow?: boolean;
}

/** Speak text with the voices from the speech settings. */
export function speak(text: string, lang: string | null | undefined, o: SpeakOptions) {
  const owner = o.slow ? `${o.owner}:slow` : o.owner;
  if (!text.trim()) return;
  play(owner, async () => {
    const l = lang || (await commands.detectLanguage(text)) || useSettings().settings.value.languages.primary;
    return unwrap(commands.speak({ text, lang: l, slow: !!o.slow }));
  });
}

/** Speak with a specific voice (settings previews). */
export function previewVoice(text: string, engine: Engine, voice: string, owner: string) {
  play(owner, () => unwrap(commands.speakPreview(text, engine, voice)));
}

/** Speak with the voice the settings would use for `lang`. */
export function previewLanguage(text: string, lang: string, owner: string) {
  play(owner, () => unwrap(commands.speak({ text, lang, slow: false })));
}

/** A short phrase per language for voice previews. */
export const SAMPLES: Record<string, string> = {
  en: 'Hello! How are you today?',
  ru: 'Привет! Как у тебя дела сегодня?',
  es: '¡Hola! ¿Qué tal estás hoy?',
  de: 'Hallo! Wie geht es dir heute?',
  fr: 'Bonjour ! Comment vas-tu aujourd’hui ?',
  it: 'Ciao! Come stai oggi?',
  pt: 'Olá! Como você está hoje?',
  uk: 'Привіт! Як у тебе справи сьогодні?',
  be: 'Прывітанне! Як у цябе справы сёння?',
  pl: 'Cześć! Jak się dzisiaj masz?',
  cs: 'Ahoj! Jak se dnes máš?',
  nl: 'Hallo! Hoe gaat het vandaag met je?',
  sv: 'Hej! Hur mår du idag?',
  da: 'Hej! Hvordan har du det i dag?',
  nb: 'Hei! Hvordan har du det i dag?',
  fi: 'Hei! Mitä sinulle kuuluu tänään?',
  el: 'Γεια σου! Πώς είσαι σήμερα;',
  hu: 'Szia! Hogy vagy ma?',
  ro: 'Bună! Ce mai faci astăzi?',
  bg: 'Здравей! Как си днес?',
  sr: 'Здраво! Како си данас?',
  hr: 'Bok! Kako si danas?',
  tr: 'Merhaba! Bugün nasılsın?',
  kk: 'Сәлем! Бүгін қалайсың?',
  uz: 'Salom! Bugun qalaysiz?',
  ka: 'გამარჯობა! როგორ ხარ დღეს?',
  hy: 'Բարև։ Ինչպե՞ս ես այսօր։',
  he: 'שלום! מה שלומך היום?',
  ar: 'مرحبًا! كيف حالك اليوم؟',
  fa: 'سلام! امروز حالت چطور است؟',
  hi: 'नमस्ते! आज आप कैसे हैं?',
  zh: '你好！你今天怎么样？',
  ja: 'こんにちは！今日の調子はどうですか？',
  ko: '안녕하세요! 오늘 기분이 어떠세요?',
  vi: 'Xin chào! Hôm nay bạn thế nào?',
  th: 'สวัสดี! วันนี้คุณเป็นอย่างไรบ้าง?',
  id: 'Halo! Apa kabar hari ini?',
};
