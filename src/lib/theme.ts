import { watch } from 'vue';
import { getCurrentWebview } from '@tauri-apps/api/webview';
import { commands } from './api';
import { useSettings } from './settings';

const media = window.matchMedia('(prefers-color-scheme: dark)');

/** Apply theme, glass mode, interface zoom and text size to this window. */
export async function initTheme() {
  const { settings } = useSettings();
  const root = document.documentElement;
  const webview = getCurrentWebview();

  const applyGlass = async () => {
    root.classList.toggle('glass', await commands.windowGlass());
  };

  const apply = () => {
    const mode = settings.value.general.theme;
    const dark = mode === 'dark' || (mode === 'system' && media.matches);
    root.classList.toggle('dark', dark);
    root.style.setProperty('--text-size', `${settings.value.general.font_size}px`);
  };

  const applyZoom = () => {
    const scale = Math.min(200, Math.max(50, settings.value.general.ui_scale)) / 100;
    webview.setZoom(scale).catch(console.error);
  };

  apply();
  applyZoom();
  await applyGlass();
  media.addEventListener('change', apply);
  watch(() => [settings.value.general.theme, settings.value.general.font_size], apply);
  watch(() => settings.value.general.ui_scale, applyZoom);
  watch(
    () => [settings.value.general.window_effect, settings.value.general.theme],
    () => setTimeout(applyGlass, 50),
  );
}
