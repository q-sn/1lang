import { ref } from 'vue';

export type MainView = 'translate' | 'check' | 'history' | 'settings';
export const MAIN_VIEWS: MainView[] = ['translate', 'check', 'history', 'settings'];

/** Navigation of the main window, shared by its views. */
export const mainView = ref<MainView>('translate');
export const settingsSection = ref('general');
/** Text handed to the check view (from history); it takes it once shown. */
export const checkText = ref<string | null>(null);

/** "settings", "settings/providers", "check", … */
export function navigate(target: string) {
  const [view, section] = target.split('/') as [MainView, string | undefined];
  if (!MAIN_VIEWS.includes(view)) return;
  if (section) settingsSection.value = section;
  mainView.value = view;
}

export function openCheck(text: string) {
  checkText.value = text;
  mainView.value = 'check';
}
