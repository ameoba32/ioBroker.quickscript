'use strict';

/**
 * api.js — Clean SDK for QuickScript automations with automatic resource tracking & hot-reload cleanup.
 *
 * Usage in any script:
 *   import { getState, setState, on, schedule, sendTo, log, setTimeout, clearTimeout } from 'iobroker.quickscript';
 */

const cron = require('node-cron');

let activeAdapter = null;
const activeListeners = []; // [{ event, fn }]
const activeCronTasks = []; // [ScheduledTask]
const activeTimers = new Set(); // Set<{ type: 'timeout'|'interval', handle }>

function setActiveAdapter(adapter) {
    activeAdapter = adapter;
    if (adapter && typeof adapter.setMaxListeners === 'function') {
        adapter.setMaxListeners(0);
    }
}

function getAdapter() {
    return activeAdapter;
}

/**
 * Automatically called by main.js before hot-reloading scripts or unloading the adapter.
 * Stops all previous stateChange listeners, cron jobs, and timers so scripts never need manual cleanup code.
 */
function cleanupAll() {
    if (activeAdapter) {
        for (const item of activeListeners) {
            activeAdapter.removeListener(item.event, item.fn);
        }
    }
    activeListeners.length = 0;

    for (const task of activeCronTasks) {
        try {
            task.stop();
        } catch {
            // ignore
        }
    }
    activeCronTasks.length = 0;

    for (const item of activeTimers) {
        if (item.type === 'interval') {
            clearInterval(item.handle);
        } else {
            clearTimeout(item.handle);
        }
    }
    activeTimers.clear();
}

function log(msg, level = 'info') {
    if (activeAdapter && activeAdapter.log && typeof activeAdapter.log[level] === 'function') {
        activeAdapter.log[level](String(msg));
    } else {
        console.log(msg);
    }
}

async function getState(id) {
    if (!activeAdapter) return { val: null, ack: false };
    return (await activeAdapter.getForeignStateAsync(String(id))) || { val: null, ack: false };
}

async function setState(id, val, ack = false) {
    if (!activeAdapter) return;
    const stateObj =
        typeof val === 'object' && val !== null && 'val' in val
            ? val
            : { val, ack: Boolean(ack) };
    await activeAdapter.setForeignStateAsync(String(id), stateObj);
}

function patternToRegex(pattern) {
    if (pattern instanceof RegExp) return pattern;
    const escaped = String(pattern)
        .replace(/[.+?^${}()|[\]\\]/g, '\\$&')
        .replace(/\*/g, '.*');
    return new RegExp(`^${escaped}$`);
}

function on(patternOrOpts, callback) {
    if (!activeAdapter) return;

    const isObj =
        typeof patternOrOpts === 'object' &&
        patternOrOpts !== null &&
        !(patternOrOpts instanceof RegExp) &&
        !Array.isArray(patternOrOpts);
    const rawPatterns = isObj ? patternOrOpts.id : patternOrOpts;
    const ackFilter = isObj && typeof patternOrOpts.ack === 'boolean' ? patternOrOpts.ack : undefined;

    const patterns = Array.isArray(rawPatterns) ? rawPatterns : [rawPatterns];
    const regexes = patterns.map((p) => patternToRegex(p));

    // Subscribe only to the requested state patterns in ioBroker
    for (const p of patterns) {
        if (typeof p === 'string') {
            activeAdapter.subscribeForeignStates(p);
        }
    }

    const listener = (id, state) => {
        if (!state) return;
        if (ackFilter !== undefined && state.ack !== ackFilter) return;
        if (!regexes.some((rx) => rx.test(id))) return;

        callback({
            id,
            state,
            val: state.val,
            ack: state.ack,
            ts: state.ts,
            lc: state.lc,
            from: state.from,
        });
    };

    activeAdapter.on('stateChange', listener);
    activeListeners.push({ event: 'stateChange', fn: listener });
}

function schedule(cronExpr, callback) {
    const task = cron.schedule(cronExpr, callback);
    activeCronTasks.push(task);
    return task;
}

function sendTo(instance, command, message, callback) {
    if (!activeAdapter) return;
    activeAdapter.sendTo(instance, command, message, callback);
}

function trackedSetTimeout(fn, ms, ...args) {
    const entry = { type: 'timeout', handle: null };
    const timerFn = activeAdapter && typeof activeAdapter.setTimeout === 'function'
        ? activeAdapter.setTimeout.bind(activeAdapter)
        : globalThis.setTimeout;
    entry.handle = timerFn(() => {
        activeTimers.delete(entry);
        fn(...args);
    }, ms);
    activeTimers.add(entry);
    return entry;
}

function trackedClearTimeout(entryOrHandle) {
    if (!entryOrHandle) return;
    const clearFn = activeAdapter && typeof activeAdapter.clearTimeout === 'function'
        ? activeAdapter.clearTimeout.bind(activeAdapter)
        : globalThis.clearTimeout;
    if (typeof entryOrHandle === 'object' && 'handle' in entryOrHandle) {
        clearFn(entryOrHandle.handle);
        activeTimers.delete(entryOrHandle);
    } else {
        clearFn(entryOrHandle);
    }
}

function trackedSetInterval(fn, ms, ...args) {
    const entry = { type: 'interval', handle: null };
    const intervalFn = activeAdapter && typeof activeAdapter.setInterval === 'function'
        ? activeAdapter.setInterval.bind(activeAdapter)
        : globalThis.setInterval;
    entry.handle = intervalFn(() => {
        activeTimers.delete(entry);
        fn(...args);
    }, ms);
    activeTimers.add(entry);
    return entry;
}

function trackedClearInterval(entryOrHandle) {
    if (!entryOrHandle) return;
    const clearFn = activeAdapter && typeof activeAdapter.clearInterval === 'function'
        ? activeAdapter.clearInterval.bind(activeAdapter)
        : globalThis.clearInterval;
    if (typeof entryOrHandle === 'object' && 'handle' in entryOrHandle) {
        clearFn(entryOrHandle.handle);
        activeTimers.delete(entryOrHandle);
    } else {
        clearFn(entryOrHandle);
    }
}

module.exports = {
    setActiveAdapter,
    cleanupAll,
    getAdapter,
    cron,
    log,
    getState,
    setState,
    on,
    subscribe: on,
    schedule,
    sendTo,
    setTimeout: trackedSetTimeout,
    clearTimeout: trackedClearTimeout,
    setInterval: trackedSetInterval,
    clearInterval: trackedClearInterval,
};
