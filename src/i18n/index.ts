import { watch } from 'vue';
import { createI18n } from 'vue-i18n';
import en from './en';
import ru from './ru';
import es from './es';
import { useSettings } from '@/lib/settings';

export const UI_LANGUAGES = ['en', 'ru', 'es'] as const;

export const i18n = createI18n({
  legacy: false,
  locale: 'en',
  fallbackLocale: 'en',
  messages: { en, ru, es },
  // Messages contain JSON examples with braces; they are escaped where needed.
  warnHtmlMessage: false,
});

type UiLanguage = (typeof UI_LANGUAGES)[number];

function resolve(pref: string, system: string): UiLanguage {
  const lang = pref === 'system' ? system : pref;
  return (UI_LANGUAGES as readonly string[]).includes(lang) ? (lang as UiLanguage) : 'en';
}

/** Follow the UI language from settings ("system" = OS language). */
export function initLocale() {
  const { settings, appInfo } = useSettings();
  const apply = () => {
    const l = resolve(settings.value.general.ui_language, appInfo.value?.system_language ?? 'en');
    i18n.global.locale.value = l;
    document.documentElement.lang = l;
  };
  apply();
  watch(() => settings.value.general.ui_language, apply);
}
