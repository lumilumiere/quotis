import { test } from 'node:test';
import assert from 'node:assert/strict';
import fs from 'node:fs';
import * as connection from '../src/connection.js';

// Exercise the actual entry point with deterministic IPC timing; no browser or account.
const source = fs.readFileSync(new URL('../src/settings.js', import.meta.url), 'utf8').replace(/^import .*;\r?\n/gm, '');
async function initialize(event, usage) {
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
  const document = { documentElement:new Element(), createElement:() => new Element(), getElementById:get, querySelectorAll:() => [], addEventListener() {} };
  const provider = { id:'fake', name:'Fake', enabled:true, found:true, default_enabled:true, path:'synthetic', hint:'' };
  const settings = { providers:{ fake:{ enabled:true, dir:'' } }, opacity:80, theme:'light' };
  const invoke = async name => ({ get_settings:settings, get_providers:[provider], list_displays:[], get_usage:usage })[name];
  let callback;
  const listen = async (_, handler) => { callback = handler; if (event) callback({ payload:event }); return () => {}; };
  const AsyncFunction = Object.getPrototypeOf(async function () {}).constructor;
  const dependencies = { document, invoke, listen, emit:() => {}, getCurrentWindow:() => ({ close() {} }), ...connection, addEventListener:() => {}, matchMedia:() => ({ matches:false }) };
  await new AsyncFunction(...Object.keys(dependencies), source)(...Object.values(dependencies));
  return { badge:() => get('st-fake').textContent, event:snapshot => callback({ payload:snapshot }) };
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
