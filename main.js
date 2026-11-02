'use strict';

const utils = require('@iobroker/adapter-core');
const fs = require('node:fs');
const path = require('node:path');
const Module = require('node:module');
const { pathToFileURL } = require('node:url');
const api = require('./api.js');

let currentReloadId = Date.now();

if (typeof Module.registerHooks === 'function') {
    Module.registerHooks({
        resolve(specifier, context, nextResolve) {
            if (
                specifier === 'iobroker.quickscript' ||
                specifier === 'iobroker.quickscript/api.js' ||
                specifier === 'iobroker.quickscript/api'
            ) {
                return { url: pathToFileURL(path.join(__dirname, 'api.js')).href, shortCircuit: true };
            }
            // Append ?t=<currentReloadId> to all relative imports (e.g. './lib/heating-lib.js')
            // so modified libraries in subdirectories are always re-imported fresh on hot-reload!
            if (
                (specifier.startsWith('./') || specifier.startsWith('../')) &&
                context.parentURL &&
                context.parentURL.startsWith('file://') &&
                !context.parentURL.includes('/node_modules/')
            ) {
                const resolved = nextResolve(specifier, context);
                const cleanUrl = resolved.url.split('?')[0];
                return { ...resolved, url: `${cleanUrl}?t=${currentReloadId}`, shortCircuit: true };
            }
            return nextResolve(specifier, context);
        },
    });
}

class QuickScript extends utils.Adapter {
    constructor(options = {}) {
        super({
            ...options,
            name: 'quickscript',
        });

        api.setActiveAdapter(this);
        this.scriptsDir = '';
        this.watcher = null;
        this.reloadTimer = null;

        this.on('ready', this.onReady.bind(this));
        this.on('unload', this.onUnload.bind(this));
    }

    async onReady() {
        const configuredDir =
            this.config && typeof this.config.scriptsDir === 'string'
                ? this.config.scriptsDir.trim()
                : '';

        // Automatically resolve default ioBroker data directory if not set in UI
        this.scriptsDir = configuredDir || path.join(utils.getAbsoluteDefaultDataDir(), 'quickscripts');

        if (!fs.existsSync(this.scriptsDir)) {
            fs.mkdirSync(this.scriptsDir, { recursive: true });
            this.log.info(`Created scripts directory: ${this.scriptsDir}`);
        }

        await this.reloadAllScripts();
        this.startWatcher();

        await this.setStateAsync('info.connection', true, true);
    }

    async reloadAllScripts() {
        // 1. Clean up all previous listeners, cron jobs, and timers automatically
        api.cleanupAll();
        currentReloadId = Date.now();

        let files = [];
        try {
            // Only load top-level *.js / *.mjs files in scriptsDir (ignore subdirectories).
            // Libraries in subdirectories (e.g. ./lib/heating-lib.js) are loaded when imported and get the new ?t=currentReloadId.
            files = fs
                .readdirSync(this.scriptsDir, { withFileTypes: true })
                .filter(
                    (dirent) =>
                        dirent.isFile() &&
                        (dirent.name.endsWith('.js') || dirent.name.endsWith('.mjs')) &&
                        !dirent.name.startsWith('.')
                )
                .map((dirent) => dirent.name)
                .sort();
        } catch (err) {
            this.log.error(`Failed to read scripts directory ${this.scriptsDir}: ${err.message}`);
            return;
        }

        let loaded = 0;
        for (const file of files) {
            const fullPath = path.join(this.scriptsDir, file);
            try {
                const fileUrl = `${pathToFileURL(fullPath).href}?t=${currentReloadId}`;
                await import(fileUrl);
                loaded++;
                this.log.info(`Loaded script: ${file}`);
            } catch (err) {
                this.log.error(`Error in ${file}: ${err.stack || err.message}`);
            }
        }

        await this.setStateAsync('info.scriptsLoaded', loaded, true);
    }

    startWatcher() {
        if (this.watcher) this.watcher.close();
        try {
            // Watch both root scripts and subdirectories (e.g. ./lib/) for hot-reload
            this.watcher = fs.watch(this.scriptsDir, { persistent: false, recursive: true }, (eventType, filename) => {
                if (filename && !filename.endsWith('.js') && !filename.endsWith('.mjs')) return;
                if (this.reloadTimer) this.clearTimeout(this.reloadTimer);
                this.reloadTimer = this.setTimeout(() => {
                    this.log.info(`Detected change in ${filename || this.scriptsDir}, hot-reloading scripts...`);
                    this.reloadAllScripts();
                }, 200);
            });
        } catch (err) {
            this.log.error(`Failed to watch ${this.scriptsDir}: ${err.message}`);
        }
    }

    onUnload(callback) {
        try {
            if (this.watcher) this.watcher.close();
            if (this.reloadTimer) this.clearTimeout(this.reloadTimer);
            api.cleanupAll();
            this.setState('info.connection', false, true);
            callback();
        } catch {
            callback();
        }
    }
}

function startAdapter(options) {
    return new QuickScript(options);
}

if (require.main !== module) {
    module.exports = startAdapter;
} else {
    startAdapter();
}

Object.assign(module.exports, api);
