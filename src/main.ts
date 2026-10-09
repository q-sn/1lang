import './styles/main.css';
import { createApp, type Component } from 'vue';
import { getCurrentWebviewWindow } from '@tauri-apps/api/webviewWindow';
import { i18n, initLocale } from './i18n';
import { initSettings } from './lib/settings';
import { initTheme } from './lib/theme';

const label = getCurrentWebviewWindow().label;

const roots: Record<string, () => Promise<{ default: Component }>> = {
  main: () => import('./windows/main/MainWindow.vue'),
  popup: () => import('./windows/popup/PopupWindow.vue'),
  icon: () => import('./windows/icon/IconWindow.vue'),
  region: () => import('./windows/region/RegionWindow.vue'),
};

const kind = label.startsWith('region-') ? 'region' : label;
// The icon and region pickers draw everything themselves on a transparent window.
if (kind === 'icon' || kind === 'region') document.documentElement.classList.add('bare');

// No browser context menu / reload outside text fields.
document.addEventListener('contextmenu', (e) => {
  const t = e.target as HTMLElement;
  if (!t.closest('input, textarea, .selectable')) e.preventDefault();
});
if (!import.meta.env.DEV) {
  document.addEventListener('keydown', (e) => {
    if (e.key === 'F5' || (e.ctrlKey && ['r', 'p', 'u'].includes(e.key.toLowerCase()))) e.preventDefault();
  });
}

async function boot() {
  await initSettings();
  initLocale();
  if (kind !== 'icon' && kind !== 'region') await initTheme();
  const root = await (roots[kind] ?? roots.main)();
  createApp(root.default).use(i18n).mount('#app');
}

boot().catch((e) => {
  console.error(e);
  document.body.textContent = String(e);
});
