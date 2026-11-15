// Pure view logic for the overlay: what to say about a snapshot. No DOM, no Tauri,
// so it runs under `node --test`.

const compact = new Intl.NumberFormat("en", { notation: "compact", maximumFractionDigits: 1 });
const money = { usd: new Intl.NumberFormat("en", { style: "currency", currency: "USD" }), cny: new Intl.NumberFormat("en", { style: "currency", currency: "CNY" }) };

/** Providers worth a row: anything that is connected or has a problem to report. */
export const visible = (snap) => (snap?.providers ?? []).filter((p) => p.status === "connected" || p.status === "error");

/** Remaining share of an allowance in percent, or null when there is no allowance to divide by. */
export function percentLeft(m) {
  if (m.category !== "provider_quota") return null;
  if (m.unit === "percent" && Number.isFinite(m.remaining)) return clamp(m.remaining);
  if (Number.isFinite(m.remaining) && m.total > 0) return clamp((m.remaining / m.total) * 100);
  return null;
}

const clamp = (n) => Math.min(100, Math.max(0, n));

/** ok / warn / bad by how much is left; "unknown" when the number can no longer be trusted. */
export function level(m, warnBelow = 20) {
  const left = percentLeft(m);
  if (left == null || m.freshness === "expired") return "unknown";
  if (left <= warnBelow / 2) return "bad";
  return left <= warnBelow ? "warn" : "ok";
}

export function amount(value, unit) {
  if (!Number.isFinite(value)) return "–";
  if (unit === "percent") return `${Math.round(value)}%`;
  if (unit === "usd" || unit === "cny") return money[unit].format(value);
  return compact.format(value);
}

/** "2d 3h", "4h 10m", "12m", or "" once the time has passed. */
export function until(ts, now) {
  const s = ts - now;
  if (!ts || s <= 0) return "";
  const d = Math.floor(s / 86400), h = Math.floor((s % 86400) / 3600), m = Math.floor((s % 3600) / 60);
  return d ? `${d}d ${h}h` : h ? `${h}h ${m}m` : `${Math.max(1, m)}m`;
}

/**
 * The text for one metric row. An expired window never shows its old number as current,
 * and never shows an invented "100% left".
 */
export function describe(m, now, showUsed = false) {
  const left = percentLeft(m);
  if (m.freshness === "expired") return { value: "reset", detail: "updating…", stale: true };
  const reset = m.resets_at ? until(m.resets_at, now) : "";
  const stale = m.freshness === "stale";
  const tail = [reset, stale ? "stale" : ""].filter(Boolean).join(" · ");
  if (left != null) {
    const pct = showUsed ? 100 - left : left;
    const exact = m.unit !== "percent" && Number.isFinite(m.remaining) ? ` (${amount(m.remaining, m.unit)})` : "";
    return { value: `${Math.round(pct)}%`, detail: `${showUsed ? "used" : "left"}${exact}${tail ? ` · ${tail}` : ""}`, stale };
  }
  if (m.category === "provider_quota" && Number.isFinite(m.remaining)) return { value: amount(m.remaining, m.unit), detail: `left${tail ? ` · ${tail}` : ""}`, stale };
  const est = m.category === "estimated_cost" ? "est. " : "";
  return { value: amount(m.used, m.unit), detail: `${est}in ${m.label}${stale ? " · stale" : ""}`, stale };
}

/** The tightest trustworthy quota of a provider: the metric a glance should show. */
export function tightest(p) {
  return p.metrics
    .filter((m) => percentLeft(m) != null && m.freshness !== "expired")
    .reduce((worst, m) => (worst == null || percentLeft(m) < percentLeft(worst) ? m : worst), null);
}

const ERROR_STATE = { offline: "offline", permission_denied: "denied", not_signed_in: "disconnected", auth_expired: "disconnected" };

/**
 * Everything the collapsed pill shows.
 * state: none | ok | warning | disconnected | error | offline | denied
 */
export function summarize(snap, settings = {}, now = Date.now() / 1000) {
  const warnBelow = settings.warn_below ?? 20;
  const all = snap?.providers ?? [];
  const shown = visible(snap);
  const refreshing = all.some((p) => p.refreshing);
  if (!shown.length) return { state: "none", name: "Quotis", value: "No tools", level: "unknown", refreshing, others: [], title: "No tools connected. Open Settings to connect one." };

  const pinned = all.find((p) => p.id === settings.pill_provider && p.status !== "disabled");
  const ranked = shown
    .map((p) => ({ p, m: tightest(p) }))
    .filter((x) => x.m)
    .sort((a, b) => percentLeft(a.m) - percentLeft(b.m));
  const pick = pinned ? { p: pinned, m: tightest(pinned) } : ranked[0] ?? { p: shown[0], m: null };
  const { p, m } = pick;

  let state = "ok";
  let value;
  let lvl = "unknown";
  const errors = shown.filter((x) => x.error);
  if (errors.length && errors.length === shown.length && errors.every((x) => x.error.kind === "offline")) state = "offline";
  else if (p.status === "not_detected") state = "disconnected";
  else if (p.error && !m) state = ERROR_STATE[p.error.kind] ?? "error";

  if (m) {
    const d = describe(m, now, settings.show_used);
    value = d.value;
    lvl = level(m, warnBelow);
    if (state === "ok" && warnBelow > 0 && (lvl === "warn" || lvl === "bad")) state = "warning";
  } else if (state !== "ok") {
    value = { offline: "Offline", denied: "No access", disconnected: "Sign in", error: "Error" }[state];
  } else {
    const first = p.metrics.find((x) => x.freshness !== "expired");
    value = first ? describe(first, now, settings.show_used).value : p.notes[0] ? "OK" : "…";
  }

  const others = shown
    .filter((x) => x.id !== p.id)
    .slice(0, 5)
    .map((x) => {
      const t = tightest(x);
      return { id: x.id, level: t ? level(t, warnBelow) : x.error ? "bad" : "unknown" };
    });
  const what = m ? `${m.label} ${describe(m, now, settings.show_used).detail}` : p.error?.message ?? p.notes[0] ?? "";
  return { state, name: p.name, value, level: lvl, refreshing, others, title: `${p.name}: ${value} ${what}`.trim(), provider: p.id };
}
