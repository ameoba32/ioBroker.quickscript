'use strict';

/**
 * compat.js — Optional compatibility layer for scripts migrated from `iobroker.javascript`.
 *
 * Usage in any script:
 *   import { getState, setState, on, schedule, sendTo, log } from 'iobroker.quickscript/compat.js';
 */

const { getAdapter, cron } = require('./main.js');

const stateCache = new Map();
let cacheInitialized = false;

async function ensureCache(adapter) {
    if (cacheInitialized || !adapter) return;
    cacheInitialized = true;
    const states = await adapter.getForeignStatesAsync('*');
    if (states) {
        for (const [id, st] of Object.entries(states)) {
            if (st) stateCache.set(id, st);
        }
    }
    adapter.on('stateChange', (id, state) => {
        if (!state) stateCache.delete(id);
        else stateCache.set(id, state);
    });
}

function log(msg, level = 'info') {
    const adapter = getAdapter();
    if (adapter && adapter.log && typeof adapter.log[level] === 'function') {
        adapter.log[level](String(msg));
    } else {
        console.log(msg);
    }
}

function getState(id) {
    const adapter = getAdapter();
    ensureCache(adapter);
    return stateCache.get(String(id)) || { val: null, ack: false };
}

async function getStateAsync(id) {
    const adapter = getAdapter();
    return adapter.getForeignStateAsync(String(id));
}

function setState(id, val, ack = false, cb) {
    const adapter = getAdapter();
    const stateObj =
        typeof val === 'object' && val !== null && 'val' in val
            ? val
            : { val, ack: Boolean(ack) };
    adapter.setForeignState(String(id), stateObj, cb);
}

function setStateAsync(id, val, ack = false) {
    const adapter = getAdapter();
    const stateObj =
        typeof val === 'object' && val !== null && 'val' in val
            ? val
            : { val, ack: Boolean(ack) };
    return adapter.setForeignStateAsync(String(id), stateObj);
}

function on(patternOrOpts, callback) {
    const adapter = getAdapter();
    ensureCache(adapter);

    const isObj =
        typeof patternOrOpts === 'object' &&
        patternOrOpts !== null &&
        !(patternOrOpts instanceof RegExp) &&
        !Array.isArray(patternOrOpts);
    const targetId = isObj ? patternOrOpts.id : patternOrOpts;
    const changeMode = isObj && patternOrOpts.change ? patternOrOpts.change : 'ne';

    const regex =
        targetId instanceof RegExp
            ? targetId
            : new RegExp(
                  `^${String(targetId)
                      .replace(/[.+?^${}()|[\]\\]/g, '\\$&')
                      .replace(/\*/g, '.*')}$`
              );

    adapter.on('stateChange', (id, state) => {
        if (!state || !regex.test(id)) return;
        const oldState = stateCache.get(id) || { val: undefined };
        if (changeMode === 'ne' && oldState.val !== undefined && state.val === oldState.val) return;
        callback({
            id,
            state,
            oldState,
            val: state.val,
            oldVal: oldState.val,
            ack: state.ack,
            ts: state.ts,
            lc: state.lc,
            from: state.from,
        });
    });
}

function schedule(cronExpr, callback) {
    return cron.schedule(cronExpr, callback);
}

function sendTo(instance, command, message, callback) {
    const adapter = getAdapter();
    adapter.sendTo(instance, command, message, callback);
}

module.exports = {
    getAdapter,
    log,
    getState,
    getStateAsync,
    setState,
    setStateAsync,
    on,
    subscribe: on,
    schedule,
    sendTo,
};
