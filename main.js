'use strict';

const fs = require('node:fs');
const path = require('node:path');
const { spawn } = require('node:child_process');

const BIN_PATH = path.join(__dirname, 'bin', `quickscript-${process.platform}-${process.arch}.node`);

function resolveInstanceNumber() {
    if (process.env.IOB_INSTANCE !== undefined) {
        return String(process.env.IOB_INSTANCE);
    }
    for (let i = 2; i < process.argv.length; i++) {
        if (/^\d+$/.test(process.argv[i])) {
            return process.argv[i];
        }
    }
    return '0';
}

function startCompactBridge(options = {}) {
    const utils = require('@iobroker/adapter-core');
    const adapter = new utils.Adapter({
        ...options,
        name: 'quickscript',
    });

    let child = null;

    adapter.on('ready', () => {
        const instance = String(adapter.instance ?? 0);
        child = spawn(BIN_PATH, [instance], {
            stdio: 'inherit',
            env: {
                ...process.env,
                IOB_INSTANCE: instance,
            },
        });
    });

    adapter.on('unload', callback => {
        if (child) {
            try {
                child.kill('SIGTERM');
            } catch {
                // ignore
            }
            child = null;
        }
        callback();
    });

    return adapter;
}

if (require.main !== module) {
    module.exports = options => startCompactBridge(options);
} else {
    process.env.IOB_INSTANCE = resolveInstanceNumber();

    if (fs.existsSync(BIN_PATH)) {
        // Single hybrid binary: require() invokes napi_register_module_v1 inside the binary,
        // which immediately execv()s itself in-place, replacing Node.js under the same PID!
        require(BIN_PATH);
    } else {
        throw new Error(`Native binary not found for ${process.platform}-${process.arch}: ${BIN_PATH}`);
    }
}
