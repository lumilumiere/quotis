import { test } from "node:test";
import assert from "node:assert/strict";
import { amount, describe, level, percentLeft, summarize, tightest, until, visible } from "../src/format.js";

const NOW = 1_800_000_000;
const quota = (over = {}) => ({ id: "five_hour", label: "5h", category: "provider_quota", unit: "percent", used: 38, remaining: 62, total: 100, resets_at: NOW + 7800, observed_at: NOW, source: "provider_api", freshness: "fresh", ...over });
const tokens = (over = {}) => ({ id: "tokens_5h", label: "5h", category: "local_usage", unit: "tokens", used: 1_234_000, observed_at: NOW, source: "local_log", freshness: "fresh", ...over });
const provider = (over = {}) => ({ id: "claude", name: "Claude Code", vendor: "Anthropic", status: "connected", refreshing: false, metrics: [quota()], notes: [], ...over });
const snap = (...providers) => ({ schema_version: 1, generated_at: NOW, app_version: "2.0.0", providers });

test("percentLeft only exists for a provider-reported allowance", () => {
  assert.equal(percentLeft(quota()), 62);
  assert.equal(percentLeft(quota({ unit: "usd", remaining: 5, total: 20 })), 25);
  assert.equal(percentLeft(quota({ unit: "usd", remaining: 5, total: undefined })), null, "a balance without a total is not a percentage");
  assert.equal(percentLeft(tokens()), null, "local token counts never become a quota");
  assert.equal(percentLeft(quota({ remaining: 140 })), 100);
  assert.equal(percentLeft(quota({ remaining: -3 })), 0);
});

test("level thresholds and distrust of expired data", () => {
  assert.equal(level(quota({ remaining: 62 })), "ok");
  assert.equal(level(quota({ remaining: 20 })), "warn");
  assert.equal(level(quota({ remaining: 10 })), "bad");
  assert.equal(level(quota({ remaining: 2, freshness: "expired" })), "unknown");
  assert.equal(level(tokens()), "unknown");
  assert.equal(level(quota({ remaining: 45 }), 50), "warn");
});

test("until formats and never goes negative", () => {
  assert.equal(until(NOW + 2 * 86400 + 3 * 3600, NOW), "2d 3h");
  assert.equal(until(NOW + 4 * 3600 + 600, NOW), "4h 10m");
  assert.equal(until(NOW + 20, NOW), "1m");
  assert.equal(until(NOW - 5, NOW), "");
  assert.equal(until(0, NOW), "");
});

test("amount formats each unit", () => {
  assert.equal(amount(61.6, "percent"), "62%");
  assert.equal(amount(1_234_000, "tokens"), "1.2M");
  assert.equal(amount(3.5, "usd"), "$3.50");
  assert.equal(amount(undefined, "usd"), "–");
  assert.equal(amount(NaN, "tokens"), "–");
});

test("describe: fresh, used, stale and expired", () => {
  assert.deepEqual(describe(quota(), NOW), { value: "62%", detail: "left · 2h 10m", stale: false });
  assert.deepEqual(describe(quota(), NOW, true), { value: "38%", detail: "used · 2h 10m", stale: false });
  assert.deepEqual(describe(quota({ freshness: "stale", resets_at: undefined }), NOW), { value: "62%", detail: "left · stale", stale: true });
  // A window that reset since it was read shows neither the old number nor a made-up 100%.
  const expired = describe(quota({ freshness: "expired", resets_at: NOW - 10, remaining: 3 }), NOW);
  assert.deepEqual(expired, { value: "reset", detail: "updating…", stale: true });
  assert.ok(!JSON.stringify(expired).includes("100") && !JSON.stringify(expired).includes("3%"));
});

test("describe: money quotas, balances, local usage and estimates stay distinct", () => {
  assert.deepEqual(describe(quota({ unit: "usd", remaining: 12.5, total: 20, resets_at: undefined }), NOW), { value: "63%", detail: "left ($12.50)", stale: false });
  assert.deepEqual(describe(quota({ unit: "usd", remaining: 3.5, total: undefined, used: undefined, resets_at: undefined, label: "balance" }), NOW), { value: "$3.50", detail: "left", stale: false });
  assert.deepEqual(describe(tokens(), NOW), { value: "1.2M", detail: "in 5h", stale: false });
  assert.deepEqual(describe(tokens({ category: "estimated_cost", unit: "usd", used: 0.5, label: "24h" }), NOW), { value: "$0.50", detail: "est. in 24h", stale: false });
});

test("tightest ignores expired windows and non-quotas", () => {
  const p = provider({ metrics: [quota({ id: "a", remaining: 80 }), quota({ id: "b", remaining: 30 }), quota({ id: "c", remaining: 1, freshness: "expired" }), tokens()] });
  assert.equal(tightest(p).id, "b");
  assert.equal(tightest(provider({ metrics: [tokens()] })), null);
});

test("visible hides disabled and undetected providers", () => {
  const s = snap(provider(), provider({ id: "x", status: "disabled" }), provider({ id: "y", status: "not_detected" }), provider({ id: "z", status: "error" }));
  assert.deepEqual(visible(s).map((p) => p.id), ["claude", "z"]);
  assert.deepEqual(visible(null), []);
  assert.deepEqual(visible({}), []);
});

test("summarize: no providers", () => {
  const s = summarize(snap(provider({ status: "not_detected", metrics: [] })), {}, NOW);
  assert.equal(s.state, "none");
  assert.equal(s.value, "No tools");
  assert.equal(summarize(null, {}, NOW).state, "none");
});

test("summarize: picks the tightest quota across providers and lists the others", () => {
  const s = summarize(snap(provider(), provider({ id: "codex", name: "Codex", metrics: [quota({ remaining: 15 })] }), provider({ id: "gemini", name: "Gemini CLI", metrics: [tokens()] })), {}, NOW);
  assert.equal(s.name, "Codex");
  assert.equal(s.value, "15%");
  assert.equal(s.state, "warning");
  assert.equal(s.level, "warn");
  assert.deepEqual(s.others, [{ id: "claude", level: "ok" }, { id: "gemini", level: "unknown" }]);
});

test("summarize: pinned provider wins, show_used flips the number, alerts can be turned off", () => {
  const two = snap(provider(), provider({ id: "codex", name: "Codex", metrics: [quota({ remaining: 15 })] }));
  const pinned = summarize(two, { pill_provider: "claude", show_used: true }, NOW);
  assert.equal(pinned.name, "Claude Code");
  assert.equal(pinned.value, "38%");
  assert.equal(pinned.state, "ok");
  assert.equal(summarize(two, { warn_below: 0 }, NOW).state, "ok");
  assert.equal(summarize(two, { pill_provider: "does-not-exist" }, NOW).name, "Codex", "an unknown pin falls back to automatic");
});

test("summarize: error states", () => {
  const err = (kind, extra = {}) => provider({ status: "error", metrics: [], error: { kind, message: "m" }, ...extra });
  assert.equal(summarize(snap(err("offline")), {}, NOW).state, "offline");
  assert.equal(summarize(snap(err("offline")), {}, NOW).value, "Offline");
  assert.equal(summarize(snap(err("permission_denied")), {}, NOW).state, "denied");
  assert.equal(summarize(snap(err("auth_expired")), {}, NOW).state, "disconnected");
  assert.equal(summarize(snap(err("bad_response")), {}, NOW).state, "error");
  assert.equal(summarize(snap(err("rate_limited")), {}, NOW).state, "error");
  // One provider offline while another works is not "offline".
  assert.equal(summarize(snap(provider(), err("offline", { id: "b" })), {}, NOW).state, "ok");
  // A provider with last-known numbers and a transient error still shows its numbers.
  const degraded = summarize(snap(provider({ error: { kind: "rate_limited", message: "m" }, metrics: [quota({ freshness: "stale" })] })), {}, NOW);
  assert.equal(degraded.value, "62%");
  // A pinned provider that is not signed in says so.
  const pinnedGone = summarize(snap(provider(), provider({ id: "cursor", name: "Cursor", status: "not_detected", metrics: [] })), { pill_provider: "cursor" }, NOW);
  assert.equal(pinnedGone.state, "disconnected");
  assert.equal(pinnedGone.value, "Sign in");
});

test("summarize: usage-only provider and refreshing flag", () => {
  const s = summarize(snap(provider({ id: "gemini", name: "Gemini CLI", metrics: [tokens()], refreshing: true })), {}, NOW);
  assert.equal(s.value, "1.2M");
  assert.equal(s.state, "ok");
  assert.equal(s.refreshing, true);
  const expiredOnly = summarize(snap(provider({ metrics: [quota({ freshness: "expired" })] })), {}, NOW);
  assert.equal(expiredOnly.value, "…", "an expired-only provider shows no number at all");
  assert.equal(summarize(snap(provider({ metrics: [], notes: ["unlimited plan"] })), {}, NOW).value, "OK");
});

test("summarize: hostile provider strings pass through as plain text only", () => {
  const evil = '<img src=x onerror="alert(1)">';
  const s = summarize(snap(provider({ name: evil, metrics: [], notes: [evil] })), {}, NOW);
  assert.equal(s.name, evil, "format.js returns strings; the renderer must use textContent");
  assert.equal(typeof s.title, "string");
});
