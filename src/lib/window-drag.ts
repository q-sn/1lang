import { getCurrentWindow } from '@tauri-apps/api/window';

const INTERACTIVE = 'button, a, input, textarea, select, [role="combobox"], [role="button"], [data-no-drag]';

/**
 * Mousedown handler that moves the window when the press starts on a
 * non-interactive part of the element (title bars, headers).
 */
export function dragWindow(e: MouseEvent, opts: { maximizeOnDoubleClick?: boolean } = {}) {
  if (e.button !== 0) return;
  if ((e.target as HTMLElement).closest(INTERACTIVE)) return;
  const win = getCurrentWindow();
  if (e.detail === 2 && opts.maximizeOnDoubleClick) {
    win.toggleMaximize();
    return;
  }
  e.preventDefault();
  win.startDragging().catch(console.error);
}
