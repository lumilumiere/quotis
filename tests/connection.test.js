import { test } from 'node:test';
import assert from 'node:assert/strict';
import { connectionBadge } from '../src/connection.js';

test('current collection replaces stale discovery in Settings', () => {
  assert.equal(connectionBadge({ enabled:true, found:false }, { status:'connected', observed_at:1000 }).label, 'Connected');
});

test('detected provider awaiting its first collection is still checking', () => {
  assert.equal(connectionBadge({ enabled:true, found:true }, { status:'connected', metrics:[], notes:[], refreshing:false }).label, 'Checking');
});

test('successful notes-only collection is connected', () => {
  assert.equal(connectionBadge({ enabled:true, found:true }, { status:'connected', observed_at:1000, metrics:[], notes:['Unlimited'] }).label, 'Connected');
});

test('a connected cached reading with a current error is degraded', () => {
  assert.equal(connectionBadge({ enabled:true, found:true }, { status:'connected', error:{kind:'unavailable'} }).label, 'Issue');
});

test('disabled configuration wins over an older connected reading', () => {
  assert.equal(connectionBadge({ enabled:false, found:true }, { status:'connected' }).label, 'Off');
});

test('discovery alone does not certify a provider connection', () => {
  assert.equal(connectionBadge({ enabled:true, found:true }, null).label, 'Found');
  assert.equal(connectionBadge({ enabled:true, found:false }, { status:'not_detected' }).label, 'Not found');
});
