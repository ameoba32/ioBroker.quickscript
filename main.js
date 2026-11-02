'use strict';

const utils = require('@iobroker/adapter-core');
const fs = require('node:fs');
const path = require('node:path');
const Module = require('node:module');
const { pathToFileURL } = require('node:url');
const cron = require('node-cron');

if (typeof Module.registerHooks === 'function') {
    Module.registerHooks({
        resolve(specifier, context, nextResolve) {
            if (specifier === 'iobroker.quickscript') {
                return { url: pathToFileURL(__filename).href, shortCircuit: true };
            }
            if (specifier === 'iobroker.quickscript/compat.js' || specifier === 'iobroker.quickscript/compat') {
                return { url: pathToFileURL(path.join(__dirname, 'compat.js')).href, shortCircuit: true };
            }
            return nextResolve(specifier, context);
        },
    });
}

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
        await this.loadRootScripts();
        this.startWatcher();

        await this.setStateAsync('info.connection', true, true);
    }

    async loadRootScripts() {
        let files = [];
        try {
            // Only load top-level *.js / *.mjs files in scriptsDir (ignore subdirectories).
            // Scripts in subdirectories (e.g. ./lib/heating-lib.js) are loaded only when imported!
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
                await import(pathToFileURL(fullPath).href);
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
            // Watch both root scripts and subdirectories (e.g. ./lib/) — on any .js change, restart the adapter process cleanly
            this.watcher = fs.watch(this.scriptsDir, { persistent: false, recursive: true }, (eventType, filename) => {
                if (filename && !filename.endsWith('.js') && !filename.endsWith('.mjs')) return;
                if (this.reloadTimer) this.clearTimeout(this.reloadTimer);
                this.reloadTimer = this.setTimeout(() => {
                    this.log.info(`Detected change in ${filename || this.scriptsDir}, restarting QuickScript process...`);
                    this.restart();
                }, 300);
            });
        } catch (err) {
            this.log.error(`Failed to watch ${this.scriptsDir}: ${err.message}`);
        }
    }

    onUnload(callback) {
        try {
            if (this.watcher) this.watcher.close();
            if (this.reloadTimer) this.clearTimeout(this.reloadTimer);
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
