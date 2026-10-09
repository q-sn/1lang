import { computed } from 'vue';
import { useI18n } from 'vue-i18n';
import { useSettings } from './settings';

const cache = new Map<string, Intl.DisplayNames>();

/** Language name in the UI language, e.g. "ru" → "Русский" / "Russian". */
export function languageName(code: string, uiLocale: string): string {
  let dn = cache.get(uiLocale);
  if (!dn) {
    dn = new Intl.DisplayNames([uiLocale], { type: 'language' });
    cache.set(uiLocale, dn);
  }
  const tag = code === 'zh' ? 'zh-Hans' : code;
  const name = dn.of(tag) ?? code;
  return name.charAt(0).toLocaleUpperCase(uiLocale) + name.slice(1);
}

export function useLanguages() {
  const { locale } = useI18n();
  const { settings, appInfo } = useSettings();

  const name = (code: string | null | undefined) => (code ? languageName(code, locale.value) : '');

  /** Favorites first, then the rest alphabetically. */
  const ordered = computed(() => {
    const all = appInfo.value?.languages ?? [];
    const fav = settings.value.languages.favorites.filter((c) => all.includes(c));
    const rest = all
      .filter((c) => !fav.includes(c))
      .sort((a, b) => name(a).localeCompare(name(b), locale.value));
    return { favorites: fav, rest };
  });

  return { name, ordered };
}
