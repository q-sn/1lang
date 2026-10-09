import { ref, shallowRef, watch, type Ref } from 'vue';
import { commands, events, unwrap, type AppInfo, type FullSettings, type Settings } from './api';

/** Shared reactive settings of this window, kept in sync across windows. */
const settings: Ref<FullSettings | null> = ref(null);
const appInfo = shallowRef<AppInfo | null>(null);
const warnings = ref<string[]>([]);
let initPromise: Promise<void> | null = null;
let applyingRemote = false;
/** Local edits not yet persisted: remote updates must not overwrite them. */
let localPending = 0;

export function initSettings(): Promise<void> {
  initPromise ??= (async () => {
    const [s, info] = await Promise.all([commands.getSettings(), commands.appInfo()]);
    settings.value = s as FullSettings;
    appInfo.value = info;
    await events.settingsChanged.listen((e) => {
      if (localPending > 0) return;
      applyingRemote = true;
      settings.value = e.payload as FullSettings;
      queueMicrotask(() => (applyingRemote = false));
    });
  })();
  return initPromise;
}

export function useSettings() {
  return { settings: settings as Ref<FullSettings>, appInfo, warnings };
}

let saveTimer: number | undefined;
let waiters: { resolve: () => void; reject: (e: unknown) => void }[] = [];

/** Persist the current settings (debounced). Every caller's promise settles
 *  once the save that includes its changes is done. */
export function saveSettings(immediate = false): Promise<void> {
  return new Promise((resolve, reject) => {
    window.clearTimeout(saveTimer);
    if (!saveTimer) localPending++;
    waiters.push({ resolve, reject });
    const run = async () => {
      saveTimer = undefined;
      const batch = waiters;
      waiters = [];
      try {
        const res = await unwrap(commands.updateSettings(settings.value as Settings));
        warnings.value = res.warnings;
        batch.forEach((w) => w.resolve());
      } catch (e) {
        batch.forEach((w) => w.reject(e));
      } finally {
        localPending = Math.max(0, localPending - 1);
      }
    };
    if (immediate) run();
    else saveTimer = window.setTimeout(run, 350);
  });
}

/** Auto-save whenever the settings object is edited in this window. */
export function autoSaveSettings() {
  watch(
    settings,
    (_n, o) => {
      if (o && !applyingRemote) saveSettings().catch(console.error);
    },
    { deep: true },
  );
}
