import { nextTick, ref, watch, type Ref, type WatchSource } from 'vue';
import { currentMonitor, getCurrentWindow, LogicalSize, PhysicalPosition } from '@tauri-apps/api/window';

const MIN_HEIGHT = 190;
const MAX_HEIGHT = 480;

/**
 * Fit the popup's height to its content (short translations → small window)
 * until the user resizes it by hand.
 *
 * `root` is the whole popup, `scroller` the scrollable result box and
 * `content` the element inside it whose natural height should be visible.
 */
/** Same rule as `popup_width` in src-tauri/src/windows.rs. */
export function popupWidth(chars: number, max: number): number {
  const w = chars <= 60 ? 440 : chars <= 160 ? 520 : max;
  return Math.max(420, Math.min(w, Math.max(max, 420)));
}

export function usePopupAutosize(
  root: Ref<HTMLElement | null>,
  scroller: Ref<HTMLElement | null>,
  content: Ref<HTMLElement | null>,
  deps: WatchSource[],
  /** Desired width in CSS px for the current text. */
  preferredWidth: () => number,
) {
  const manual = ref(false);
  const win = getCurrentWindow();
  let scheduled = false;

  async function fit() {
    scheduled = false;
    if (manual.value || !root.value || !scroller.value || !content.value) return;
    const chrome = root.value.offsetHeight - scroller.value.clientHeight;
    const style = getComputedStyle(scroller.value);
    const padding = parseFloat(style.paddingTop) + parseFloat(style.paddingBottom);
    const cssHeight = Math.min(MAX_HEIGHT, Math.max(MIN_HEIGHT, chrome + content.value.offsetHeight + padding));

    // CSS px are scaled by the webview zoom; window sizes are logical px.
    const sf = await win.scaleFactor();
    const zoom = window.devicePixelRatio / sf;
    const inner = await win.innerSize();
    const width = Math.round(preferredWidth() * zoom);
    // Width first: the text re-wraps, then the height is measured again.
    if (Math.abs(inner.width / sf - width) >= 2) {
      await win.setSize(new LogicalSize(width, inner.height / sf));
      schedule();
      return;
    }
    const height = Math.round(cssHeight * zoom);
    if (Math.abs(inner.height / sf - height) < 2) return;
    await win.setSize(new LogicalSize(width, height));

    // Keep the grown window on screen.
    const [pos, monitor] = await Promise.all([win.outerPosition(), currentMonitor()]);
    if (monitor) {
      const bottom = monitor.workArea.position.y + monitor.workArea.size.height;
      const h = height * sf;
      if (pos.y + h > bottom) {
        await win.setPosition(new PhysicalPosition(pos.x, Math.max(monitor.workArea.position.y, Math.round(bottom - h))));
      }
    }
  }

  function schedule() {
    if (scheduled) return;
    scheduled = true;
    nextTick(() => requestAnimationFrame(() => fit().catch(console.error)));
  }

  watch(deps, schedule);
  return {
    manual,
    schedule,
    /** Call when a new text is shown. */
    reset() {
      manual.value = false;
      schedule();
    },
  };
}
