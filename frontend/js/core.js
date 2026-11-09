// Core utilities, Tauri IPC bridge, and UI notifications
const T = window.__TAURI__;

export const hasTauri = Boolean(T && T.core && T.core.invoke);

export const invoke = async (cmd, args = {}) => {
  if (!hasTauri) {
    throw new Error("Backend Vantage tidak tersedia. Jalankan aplikasi desktop untuk mengakses data perangkat.");
  }
  try {
    return await T.core.invoke(cmd, args);
  } catch (e) {
    console.warn(`[Tauri] ${cmd} error:`, e);
    throw e;
  }
};

export const listen = (event, callback) => {
  if (hasTauri && T.event && T.event.listen) {
    return T.event.listen(event, callback);
  }
  return Promise.resolve(() => {});
};

export const $ = (id) => document.getElementById(id);

export const fmt = (n, digits = 1) =>
  n === null || n === undefined || Number.isNaN(n) ? "--" : Number(n).toFixed(digits);

export const clampPct = (n) => Math.max(0, Math.min(100, n));

export function escapeHtml(text) {
  return String(text).replace(/[&<>"']/g, (c) =>
    ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" }[c])
  );
}

export function toast(message, isError = false) {
  const el = $("toast-notice");
  const txt = $("toast-text");
  if (!el || !txt) return;
  txt.textContent = message;
  el.classList.toggle("err", isError);
  el.classList.add("show");
  clearTimeout(el._timer);
  el._timer = setTimeout(() => el.classList.remove("show"), 3200);
}

export async function run(cmd, args, okMessage) {
  try {
    const res = await invoke(cmd, args);
    if (res && res.ok === false) {
      toast(res.message || "Aksi gagal", true);
      return null;
    }
    if (okMessage) toast(okMessage);
    return res;
  } catch (e) {
    toast(e?.message || String(e), true);
    return null;
  }
}

// Modal dialog helper
export function showModal(title, contentHtml, onOk) {
  const modal = $("app-modal");
  const titleEl = $("modal-title");
  const bodyEl = $("modal-body");
  const btnOk = $("modal-btn-ok");
  const btnCancel = $("modal-btn-cancel");
  if (!modal) return;

  titleEl.textContent = title;
  bodyEl.innerHTML = contentHtml;
  modal.classList.add("active");

  const close = () => {
    modal.classList.remove("active");
  };

  btnCancel.onclick = close;
  btnOk.onclick = async () => {
    if (onOk) await onOk();
    close();
  };
}
