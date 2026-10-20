// App-wide toasts: short confirmations in the corner, optionally with one action (e.g. Undo).
// Rendered once by <Toaster /> in the admin layout.

export type ToastKind = "ok" | "info" | "error";
export type Toast = {
  id: number;
  message: string;
  kind: ToastKind;
  action?: { label: string; run: () => void | Promise<void> };
};

export const toasts = $state<Toast[]>([]);
let seq = 0;

export function dismiss(id: number) {
  const i = toasts.findIndex((t) => t.id === id);
  if (i >= 0) toasts.splice(i, 1);
}

export function toast(
  message: string,
  opts: { kind?: ToastKind; action?: Toast["action"]; ms?: number } = {},
) {
  const id = ++seq;
  toasts.push({ id, message, kind: opts.kind ?? "ok", action: opts.action });
  setTimeout(() => dismiss(id), opts.ms ?? (opts.action ? 7000 : opts.kind === "error" ? 8000 : 3500));
  return id;
}

/** The API's error message for a failed response, or a generic one with the status. */
export async function errorDetail(res: Response, fallback = "Something went wrong"): Promise<string> {
  const body = await res.json().catch(() => ({}));
  return typeof body.detail === "string" ? body.detail : `${fallback} (${res.status})`;
}

/**
 * Copy text and say so; when the browser refuses the clipboard, show the text instead so it can be
 * copied by hand. Returns whether it reached the clipboard.
 */
export async function copyWithToast(text: string, label = "Link copied"): Promise<boolean> {
  try {
    await navigator.clipboard.writeText(text);
    toast(label);
    return true;
  } catch {
    toast(text, { kind: "info", ms: 10000 });
    return false;
  }
}
