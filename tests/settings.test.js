import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import * as connection from '../src/connection.js';

// Exercise the actual entry point with deterministic IPC timing; no browser or account.
const source = fs.readFileSync(new URL('../src/settings.js', import.meta.url), 'utf8').replace(/^import .*;\r?\n/gm, '');
async function initialize(event, usage, saves = false) {
  const elements = new Map();
  class Element {
    dataset = {};
    children = [];
    append(...items) { this.children.push(...items); }
    replaceChildren(...items) { this.children = items; }
    setAttribute(key, value) { this[key] = value; if (key === 'id') elements.set(value, this); }
    addEventListener() {}
  }
  const get = id => { if (!elements.has(id)) elements.set(id, new Element()); return elements.get(id); };
  const toggle = Object.assign(new Element(), { type:'checkbox', checked:true, dataset:{ id:'fake', field:'enabled' } });
  let change;
  const document = { documentElement:new Element(), createElement:() => new Element(), getElementById:get, querySelectorAll:() => saves ? [toggle] : [], addEventListener(name, handler) { if (name === 'change') change = handler; } };
  const provider = { id:'fake', name:'Fake', enabled:true, found:true, default_enabled:true, path:'synthetic', hint:'' };
  const settings = { providers:{ fake:{ enabled:true, dir:'' } }, opacity:80, theme:'light' };
  const pending = [];
  const invoke = async (name, args) => {
    if (name === 'save_settings') return new Promise((resolve, reject) => pending.push({ resolve, reject, settings:structuredClone(args.settings) }));
    return ({ get_settings:settings, get_providers:[provider], list_displays:[], get_usage:usage })[name];
  };
  let callback;
  let closeCalls = 0;
  let keydown;
  const listen = async (_, handler) => { callback = handler; if (event) callback({ payload:event }); return () => {}; };
  const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
  const dependencies = { document, invoke, listen, emit:() => {}, getCurrentWindow:() => ({ close() { closeCalls++; } }), ...connection, addEventListener:(name, handler) => { if (name === 'keydown') keydown = handler; }, matchMedia:() => ({ matches:false }), setTimeout:() => 0, clearTimeout:() => {} };
  await new AsyncFunction(...Object.keys(dependencies), source)(...Object.values(dependencies));
  return { badge:() => get('st-fake').textContent, event:snapshot => callback({ payload:snapshot }), change:enabled => { toggle.checked = enabled; return change(); }, pending, provider, close:() => get('close').onclick(), escape:() => keydown({ key:'Escape' }), closed:() => closeCalls };
}
const connected = { revision:1, generated_at:1000, providers:[{ id:'fake', status:'connected', observed_at:1000, metrics:[], notes:[] }] };
const failed = { revision:2, generated_at:1000, providers:[{ id:'fake', status:'error', error:{ kind:'unavailable', message:'Synthetic failure' }, metrics:[], notes:[] }] };

test('newer initial RPC error replaces older event within the same second', async () => {
  assert.equal((await initialize(connected, failed)).badge(), 'Issue');
});
test('newer event error survives older initial RPC within the same second', async () => {
  assert.equal((await initialize(failed, connected)).badge(), 'Issue');
});
test('delayed old event cannot replace current error after initialization', async () => {
  const ui = await initialize(null, failed);
  ui.event(connected);
  assert.equal(ui.badge(), 'Issue');
});
test('later successful collection clears the current error', async () => {
  const ui = await initialize(null, failed);
  ui.event({ ...connected, revision:3 });
  assert.equal(ui.badge(), 'Connected');
});
test('real discovery-only snapshot shape does not certify collection', async () => {
  const discovery = { ...connected, providers:[{ id:'fake', status:'connected', metrics:[], notes:[], refreshing:false }] };
  assert.equal((await initialize(null, discovery)).badge(), 'Checking');
});

test('rapid settings changes serialize immutable writes and their status responses', async () => {
  const ui = await initialize(null, connected, true);
  const first = ui.change(false);
  await Promise.resolve();
  const second = ui.change(true);
  await Promise.resolve();
  assert.equal(ui.pending.length, 1, 'second save must wait for the first response');
  assert.equal(ui.pending[0].settings.providers.fake.enabled, false);
  ui.event({ ...connected, revision:2, providers:[{ id:'fake', status:'disabled' }] });
  ui.pending[0].resolve([{ ...ui.provider, enabled:false }]);
  await first;
  await Promise.resolve();
  assert.equal(ui.pending.length, 2);
  assert.equal(ui.pending[1].settings.providers.fake.enabled, true);
  ui.event({ ...connected, revision:3 });
  ui.pending[1].resolve([{ ...ui.provider, enabled:true }]);
  await second;
  assert.equal(ui.badge(), 'Connected');
});

test('a failed save does not block the next queued operation or assert disabled state', async () => {
  const ui = await initialize(null, connected, true);
  const first = ui.change(false);
  await Promise.resolve();
  const second = ui.change(true);
  ui.pending[0].reject(new Error('Synthetic disk failure'));
  await first;
  await Promise.resolve();
  assert.equal(ui.badge(), 'Connected');
  assert.equal(ui.pending.length, 2);
  ui.pending[1].resolve([{ ...ui.provider, enabled:true }]);
  await second;
  assert.equal(ui.badge(), 'Connected');
});

for (const action of ['close', 'escape']) {
  test(`Settings ${action} waits for pending saves before destroying its window`, async () => {
    const ui = await initialize(null, connected, true);
    const save = ui.change(false);
    await Promise.resolve();
    const closing = ui[action]();
    assert.equal(ui.closed(), 0);
    ui.pending[0].resolve([{ ...ui.provider, enabled:false }]);
    await save;
    await closing;
    assert.equal(ui.closed(), 1);
  });
}
