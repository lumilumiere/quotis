import fs from 'node:fs';
import path from 'node:path';
import assert from 'node:assert/strict';
import { refresh } from '../src/format.js';

const root = path.resolve(process.argv[2] || 'commercial/agensi');
const sample = JSON.parse(fs.readFileSync(path.join(root, 'examples/sample-export.json'), 'utf8'));
const skill = fs.readFileSync(path.join(root, 'SKILL.md'), 'utf8').replace(/\s+/g, ' ');
const planning = fs.readFileSync(path.join(root, 'references/planning-playbook.md'), 'utf8');
const at = sample.snapshot.generated_at;
const snap = refresh(sample.snapshot, at);
const provider = id => snap.providers.find(p => p.id === id);
const metric = (id, name) => provider(id).metrics.find(m => m.id === name);
const left = m => m.unit === 'percent' ? m.remaining : m.total > 0 ? 100 * m.remaining / m.total : null;
const rankable = snap.providers.filter(p => p.status === 'connected' && !p.error && p.metrics.some(m => m.category === 'provider_quota') && p.metrics.filter(m => m.category === 'provider_quota').every(m => m.freshness === 'fresh' && !m.qualification));
const ranked = rankable.map(p => ({ id: p.id, left: Math.min(...p.metrics.filter(m => m.category === 'provider_quota').map(left).filter(v => v !== null)) })).sort((a, b) => b.left - a.left);
assert.equal(ranked[0].id, 'copilot');
assert.ok(Math.abs(ranked[0].left - 77.5) < 0.01);
assert.ok(!ranked.some(p => p.id === 'codex'));
assert.equal(metric('gemini', 'tokens_5h').total, undefined);
console.log('PASS capacity: Copilot 77.5%; stale Codex unranked; local tokens have no quota');

const projectedPercent = m => {
  const used = m.unit === 'percent' ? m.used : 100 * m.used / m.total;
  const remaining = m.resets_at - m.observed_at;
  const elapsed = m.window_seconds - remaining;
  return used * (1 + remaining / elapsed);
};
assert.ok(Math.abs(projectedPercent(metric('claude', 'five_hour')) - 85.7142857) < 0.001);
assert.ok(Math.abs(projectedPercent(metric('claude', 'seven_day')) - 72.367) < 0.1);
assert.equal(projectedPercent({ unit:'usd', used:7.5, total:20, observed_at:1000, resets_at:1500, window_seconds:1000 }), 75);
assert.ok(planning.includes('used_percent / elapsed') && planning.includes('positive `total`'));
console.log('PASS projections: 85.7% five-hour, 72.4% week; $7.50/$20 halfway projects 75%');

const later = refresh(sample.snapshot, at + 3601);
assert.equal(later.providers.find(p => p.id === 'codex').metrics[0].freshness, 'expired');
assert.equal(later.providers.find(p => p.id === 'codex').metrics[0].remaining, 12);
assert.ok(skill.includes('qualified zero') && skill.includes('historical synthetic'));
console.log('PASS historical/offline: reset is expired, no replenishment inferred, sample is disclosed');
assert.ok(skill.includes('Stop if permission is denied') && skill.includes('untrusted data'));
assert.ok(skill.includes('Exit 0 does not certify'));
console.log('PASS boundaries: approval for live checks; denied access stops; partial output is not certification');
console.log('4 offline scenario groups passed; arithmetic and instructions checked, not a marketplace-agent reliability test');
