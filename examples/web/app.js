import init, { WasmApp } from "./pkg/ui_wasm.js";

// Task 6.4: Global error reporting
window.addEventListener('error', (e) => {
  console.error('[ErrorReporter] Uncaught error:', e.message, e.filename, e.lineno);
  // In production, send to error service: fetch('/api/errors', { method: 'POST', body: JSON.stringify({...}) })
});
window.addEventListener('unhandledrejection', (e) => {
  console.error('[ErrorReporter] Unhandled promise rejection:', e.reason);
});

// Task 6.4: Telemetry object (populated in frame loop)
window.__telemetry = {
  frameTimeAvgMs: 0,
  frameTimeMaxMs: 0,
  totalFrames: 0,
  submissionCount: 0,
};

// Task 5.2: Register service worker
if ('serviceWorker' in navigator) {
  navigator.serviceWorker.register('./sw.js').catch(console.error);
}

const canvas = document.getElementById("app");
const dpr = window.devicePixelRatio || 1;

function resize() {
  canvas.width = window.innerWidth * dpr;
  canvas.height = window.innerHeight * dpr;
  canvas.style.width = `${window.innerWidth}px`;
  canvas.style.height = `${window.innerHeight}px`;
}

class AccessibilityMirror {
  constructor() {
    this.root = document.createElement('div');
    this.root.setAttribute('role', 'application');
    this.root.setAttribute('aria-label', 'GPU Forms UI');
    this.root.style.cssText = 'position:absolute;left:-9999px;width:1px;height:1px;overflow:hidden';
    document.body.appendChild(this.root);
    this.nodeMap = new Map();
  }
  update(a11y) {
    if (!a11y || !a11y.nodes) return;
    // Remove stale nodes
    for (const [id, el] of this.nodeMap) {
      if (!a11y.nodes.find(n => n.id === id)) {
        el.remove();
        this.nodeMap.delete(id);
      }
    }
    // Upsert nodes
    for (const node of a11y.nodes) {
      let el = this.nodeMap.get(node.id);
      if (!el) {
        el = document.createElement('div');
        this.root.appendChild(el);
        this.nodeMap.set(node.id, el);
      }
      if (node.role) el.setAttribute('role', node.role);
      if (node.label) el.setAttribute('aria-label', node.label);
      if (node.value !== undefined) el.setAttribute('aria-valuenow', node.value);
      if (node.checked !== undefined) el.setAttribute('aria-checked', node.checked);
      if (node.focused) el.setAttribute('aria-selected', 'true');
      if (node.disabled) el.setAttribute('aria-disabled', 'true');
    }
  }
}

/** Task 3.6: Read CSS env(safe-area-inset-*) via a hidden element. */
function readSafeAreaInsets() {
  const el = document.createElement('div');
  el.style.cssText = [
    'position:fixed',
    'top:env(safe-area-inset-top,0px)',
    'right:env(safe-area-inset-right,0px)',
    'bottom:env(safe-area-inset-bottom,0px)',
    'left:env(safe-area-inset-left,0px)',
    'width:0;height:0;pointer-events:none;visibility:hidden',
  ].join(';');
  document.body.appendChild(el);
  const cs = getComputedStyle(el);
  const top    = parseFloat(cs.top)    || 0;
  const right  = parseFloat(cs.right)  || 0;
  const bottom = parseFloat(cs.bottom) || 0;
  const left   = parseFloat(cs.left)   || 0;
  el.remove();
  return { top, right, bottom, left };
}

async function main() {
  await init();
  resize();
  const app = new WasmApp(canvas, canvas.width, canvas.height, dpr);
  window.__app = app;

  // Task 3.5: Detect prefers-reduced-motion and pass flag to WASM.
  const mql = window.matchMedia('(prefers-reduced-motion: reduce)');
  app.set_reduce_motion(mql.matches);
  mql.addEventListener('change', (e) => app.set_reduce_motion(e.matches));

  // Task 6.5: Dark mode detection
  const darkMql = window.matchMedia('(prefers-color-scheme: dark)');
  app.set_dark_mode(darkMql.matches);
  darkMql.addEventListener('change', (e) => app.set_dark_mode(e.matches));

  // Task 3.6: Read safe area insets and pass to WASM.
  const insets = readSafeAreaInsets();
  app.set_safe_area_insets(insets.top * dpr, insets.right * dpr, insets.bottom * dpr, insets.left * dpr);

  // Task 1.1: Hidden input proxy for mobile keyboards
  const hiddenInput = document.createElement('textarea');
  hiddenInput.style.cssText = 'position:absolute;opacity:0;width:1px;height:1px;pointer-events:none;top:0;left:-9999px';
  document.body.appendChild(hiddenInput);

  window.__focusHiddenInput = () => hiddenInput.focus();
  window.__blurHiddenInput = () => hiddenInput.blur();

  // Task 1.3: IME composition tracking
  let isComposing = false;

  const preventKeys = new Set(['Tab','Backspace','Delete','Enter','Space','ArrowLeft','ArrowRight','ArrowUp','ArrowDown','Home','End','PageUp','PageDown','Insert']);

  // Task 1.2: Use e.code instead of e.keyCode; Task 1.5: preventDefault for nav keys
  hiddenInput.addEventListener("keydown", (e) => {
    if (preventKeys.has(e.code)) e.preventDefault();
    app.handle_key_down(e.code, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
  });
  hiddenInput.addEventListener("keyup", (e) => {
    app.handle_key_up(e.code, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
  });
  hiddenInput.addEventListener("beforeinput", (e) => {
    // Task 1.3: gate text input on !isComposing
    if (e.data && !isComposing) {
      app.handle_text_input(e.data);
    }
  });
  hiddenInput.addEventListener("compositionstart", () => {
    isComposing = true;
    app.handle_composition_start();
  });
  hiddenInput.addEventListener("compositionupdate", (e) => app.handle_composition_update(e.data || ""));
  hiddenInput.addEventListener("compositionend", (e) => {
    isComposing = false;
    app.handle_composition_end(e.data || "");
  });
  hiddenInput.addEventListener("paste", (e) => {
    const text = e.clipboardData?.getData("text/plain") || "";
    if (text) app.handle_paste(text);
  });

  window.addEventListener("resize", () => {
    resize();
    app.resize(canvas.width, canvas.height, dpr);
  });

  // Task 1.6: Pointer capture for drag selection
  canvas.addEventListener("pointerdown", (e) => {
    canvas.setPointerCapture(e.pointerId);
    app.handle_pointer_down(e.offsetX * dpr, e.offsetY * dpr, e.button, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
  });
  canvas.addEventListener("pointerup", (e) => {
    canvas.releasePointerCapture(e.pointerId);
    app.handle_pointer_up(e.offsetX * dpr, e.offsetY * dpr, e.button, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
  });
  canvas.addEventListener("pointermove", (e) => {
    app.handle_pointer_move(e.offsetX * dpr, e.offsetY * dpr, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
  });
  // Task 1.5: preventDefault on wheel + passive: false
  canvas.addEventListener("wheel", (e) => {
    e.preventDefault();
    app.handle_wheel(e.offsetX * dpr, e.offsetY * dpr, e.deltaX, e.deltaY, e.ctrlKey, e.altKey, e.shiftKey, e.metaKey);
  }, { passive: false });

  // Note: navigator.clipboard.writeText() requires a user gesture and HTTPS.
  // Safari enforces this strictly. The execCommand fallback handles older/stricter browsers.
  // Programmatic clipboard read (navigator.clipboard.readText) is not attempted here;
  // paste is handled via the 'paste' event on the hidden input, which doesn't require permission.
  async function handleClipboard() {
    const request = app.take_clipboard_request();
    if (!request) return;
    try {
      await navigator.clipboard.writeText(request);
    } catch (e) {
      // Clipboard API requires user gesture and secure context.
      // Safari is strict about this — fall back to execCommand for compatibility.
      if (document.queryCommandSupported?.('copy')) {
        const ta = document.createElement('textarea');
        ta.value = request;
        ta.style.cssText = 'position:absolute;left:-9999px';
        document.body.appendChild(ta);
        ta.select();
        try {
          document.execCommand('copy');
        } finally {
          ta.remove();
        }
      } else {
        console.warn('[Clipboard] Copy failed:', e.message);
      }
    }
  }

  async function requestClipboardRead() {
    try {
      const perm = await navigator.permissions.query({ name: 'clipboard-read' });
      return perm.state !== 'denied';
    } catch {
      // permissions API not supported (Safari) — rely on paste event only
      return false;
    }
  }

  // Task 6.1: Hidden autofill form for password manager detection
  const autofillForm = document.createElement('form');
  autofillForm.style.cssText = 'position:absolute;left:-9999px;width:1px;height:1px;overflow:hidden';
  autofillForm.setAttribute('aria-hidden', 'true');

  const autofillEmail = document.createElement('input');
  autofillEmail.type = 'email';
  autofillEmail.autocomplete = 'email';
  autofillEmail.name = 'email';
  autofillEmail.tabIndex = -1;

  const autofillPassword = document.createElement('input');
  autofillPassword.type = 'password';
  autofillPassword.autocomplete = 'current-password';
  autofillPassword.name = 'password';
  autofillPassword.tabIndex = -1;

  autofillForm.appendChild(autofillEmail);
  autofillForm.appendChild(autofillPassword);
  document.body.appendChild(autofillForm);

  // Poll for autofill (password managers fill without firing input events)
  setInterval(() => {
    if (autofillEmail.value) {
      app.handle_autofill('email', autofillEmail.value);
      autofillEmail.value = '';
    }
    if (autofillPassword.value) {
      app.handle_autofill('password', autofillPassword.value);
      autofillPassword.value = '';
    }
  }, 200);

  // Task 1.4: Accessibility shadow DOM
  const accessibilityMirror = new AccessibilityMirror();

  // Task 4.6: Frame budget monitoring.
  const FRAME_BUDGET_MS = 12;
  const ROLLING_WINDOW = 60;
  const frameTimes = new Float64Array(ROLLING_WINDOW);
  let frameCount = 0;
  let lastTs = null;
  let maxMs = 0;

  window.__frameStats = { avgMs: 0, maxMs: 0, frameCount: 0 };

  function frame(ts) {
    // Measure delta since last frame.
    if (lastTs !== null) {
      const delta = ts - lastTs;
      frameTimes[frameCount % ROLLING_WINDOW] = delta;
      frameCount++;
      if (delta > maxMs) maxMs = delta;

      // Compute rolling average over the last min(frameCount, ROLLING_WINDOW) frames.
      const sampleCount = Math.min(frameCount, ROLLING_WINDOW);
      let sum = 0;
      for (let i = 0; i < sampleCount; i++) sum += frameTimes[i];
      const avgMs = sum / sampleCount;

      window.__frameStats = { avgMs, maxMs, frameCount };

      // Task 6.4: Update telemetry
      window.__telemetry.frameTimeAvgMs = avgMs;
      window.__telemetry.frameTimeMaxMs = maxMs;
      window.__telemetry.totalFrames = frameCount;

      if (avgMs > FRAME_BUDGET_MS) {
        console.warn(`[frame-budget] avg frame time ${avgMs.toFixed(2)}ms exceeds ${FRAME_BUDGET_MS}ms budget`);
      }
    }
    lastTs = ts;

    const a11y = app.frame(ts);
    window.__a11y = a11y;
    accessibilityMirror.update(a11y);
    handleClipboard();
    requestAnimationFrame(frame);
  }
  requestAnimationFrame(frame);
}

main();

// Task 5.3: Offline Form Submission Queue
class OfflineQueue {
  constructor() {
    this.dbReady = this._initDB();
  }
  async _initDB() {
    return new Promise((resolve, reject) => {
      const req = indexedDB.open('gpu-forms-queue', 1);
      req.onupgradeneeded = e => e.target.result.createObjectStore('submissions', { keyPath: 'id' });
      req.onsuccess = e => resolve(e.target.result);
      req.onerror = e => reject(e);
    });
  }
  async enqueue(data) {
    const db = await this.dbReady;
    const tx = db.transaction('submissions', 'readwrite');
    tx.objectStore('submissions').add({ id: crypto.randomUUID(), data, timestamp: Date.now() });
  }
  async flush() {
    if (!navigator.onLine) return;
    const db = await this.dbReady;
    const tx = db.transaction('submissions', 'readwrite');
    const store = tx.objectStore('submissions');
    const all = await new Promise(r => { const req = store.getAll(); req.onsuccess = () => r(req.result); });
    for (const item of all) {
      try {
        // In a real app, POST to server here
        console.log('[OfflineQueue] Replaying submission:', item);
        store.delete(item.id);
      } catch {}
    }
  }
}
const offlineQueue = new OfflineQueue();
window.__offlineQueue = offlineQueue;
window.addEventListener('online', () => offlineQueue.flush());

// Task 5.4: Push subscription helper
async function subscribeToPush() {
  if (!('PushManager' in window)) return;
  const reg = await navigator.serviceWorker.ready;
  // VAPID public key placeholder — replace with real key in production
  const vapidKey = 'BEl62iUYgUivxIkv69yViEuiBIa-Ib9-SkvMeAtA3LFgDzkrxZJjSgSnfckjBJuBkr3qBUYIHBQFLXYp5Nksh8U';
  try {
    const sub = await reg.pushManager.subscribe({
      userVisibleOnly: true,
      applicationServerKey: vapidKey,
    });
    window.__pushSubscription = sub;
    console.log('[Push] Subscribed:', JSON.stringify(sub));
  } catch (e) {
    console.warn('[Push] Subscription failed:', e);
  }
}
// Expose for use after user gesture
window.__subscribeToPush = subscribeToPush;

// Task 5.5: App Install Prompt
let deferredInstallPrompt = null;
window.addEventListener('beforeinstallprompt', e => {
  e.preventDefault();
  deferredInstallPrompt = e;
  // Show install banner after user has used the app a bit
  // Simple heuristic: show after 3 frames
  let frameCount = 0;
  const checkInstall = () => {
    frameCount++;
    if (frameCount === 180 && deferredInstallPrompt) { // ~3s at 60fps
      console.log('[PWA] App is installable — showing install prompt');
      // In a full implementation, show an in-canvas banner via WASM
      // For now, trigger immediately for demo
    }
    if (deferredInstallPrompt) requestAnimationFrame(checkInstall);
  };
  requestAnimationFrame(checkInstall);
});

window.__triggerInstallPrompt = async () => {
  if (!deferredInstallPrompt) return;
  deferredInstallPrompt.prompt();
  const { outcome } = await deferredInstallPrompt.userChoice;
  console.log('[PWA] Install outcome:', outcome);
  deferredInstallPrompt = null;
};
