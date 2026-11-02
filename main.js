'use strict';

const utils = require('@iobroker/adapter-core');
const fs = require('node:fs');
const path = require('node:path');
const { pathToFileURL } = require('node:url');
const cron = require('node-cron');

let activeInstance = null;

class QuickScript extends utils.Adapter {
    constructor(options = {}) {
        super({
            ...options,
            name: 'quickscript',
        });

        activeInstance = this;
        this.scriptsDir = '';
        this.watcher = null;
        this.reloadTimer = null;
        this.cleanupCallbacks = [];

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

        await this.subscribeForeignStatesAsync('*');
        await this.reloadAllScripts();
        this.startWatcher();

        await this.setStateAsync('info.connection', true, true);
    }

    onCleanup(fn) {
        this.cleanupCallbacks.push(fn);
    }

    async runCleanup() {
        for (const fn of this.cleanupCallbacks) {
            try {
                await fn();
            } catch (err) {
                this.log.error(`Cleanup error: ${err.stack || err.message}`);
            }
        }
        this.cleanupCallbacks = [];
    }

    async reloadAllScripts() {
        await this.runCleanup();

        let files = [];
        try {
            files = fs
                .readdirSync(this.scriptsDir)
                .filter((f) => (f.endsWith('.js') || f.endsWith('.mjs')) && !f.startsWith('.') && !f.startsWith('_'))
                .sort();
        } catch (err) {
            this.log.error(`Failed to read scripts directory ${this.scriptsDir}: ${err.message}`);
            return;
        }

        let loaded = 0;
        const cacheBuster = Date.now();
        for (const file of files) {
            const fullPath = path.join(this.scriptsDir, file);
            try {
                const fileUrl = `${pathToFileURL(fullPath).href}?t=${cacheBuster}`;
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
            this.watcher = fs.watch(this.scriptsDir, { persistent: false }, (eventType, filename) => {
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

    async onUnload(callback) {
        try {
            if (this.watcher) this.watcher.close();
            if (this.reloadTimer) this.clearTimeout(this.reloadTimer);
            await this.runCleanup();
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

// Export native ioBroker adapter getter + cron for scripts using `import { getAdapter, cron } from 'iobroker.quickscript'`
module.exports.getAdapter = () => activeInstance;
module.exports.cron = cron;
