# ioBroker.quickscript

![Logo](admin/quickscript.svg)

[![NPM version](https://img.shields.io/npm/v/iobroker.quickscript.svg)](https://www.npmjs.com/package/iobroker.quickscript)
[![Downloads](https://img.shields.io/npm/dm/iobroker.quickscript.svg)](https://www.npmjs.com/package/iobroker.quickscript)
![Number of Installations](https://iobroker.live/badges/quickscript-installed.svg)
![Current version in stable repository](https://iobroker.live/badges/quickscript-stable.svg)

Ultra-lightweight file-based automation script runner for **ioBroker** with **Compact Mode (`"compact": true`)** support.

Designed for users and AI agents who want to keep automation scripts as plain `.js` files on disk (or in a Git repository) with automatic hot-reload and ~1–2 MB RAM overhead instead of running a heavy in-browser IDE.

---

## Features

* **Plain files on disk / Git-friendly:** Edit `.js` files with any IDE, AI agent, or sync them via Git / Resilio Sync.
* **Compact Mode native (`"compact": true`):** Runs inside an existing ioBroker compact group without spawning an extra Node.js process (~1–2 MB RAM).
* **Hot-Reload (`fs.watch`):** Automatically cleans up previous listeners/cron jobs and reloads scripts within milliseconds whenever a `.js` file is saved.
* **Zero Global Pollution:** Clean ES Module `import` syntax — choose between direct native ioBroker `adapter` API or the optional `compat.js` helper for scripts migrated from `iobroker.javascript`.

---

## Configuration

In the instance settings (**Admin -> Instances -> quickscript.0**):
* **Scripts directory path on disk (`scriptsDir`):** Path to the folder containing your `*.js` scripts. If left empty, defaults to `<iobroker-data>/quickscripts` (e.g. `/opt/iobroker/iobroker-data/quickscripts`).

Files starting with `_` or `.` are ignored by the auto-runner so you can store shared helper modules in the same folder and import them via standard ES `import`.

---

## Usage Examples

### Option 1: Direct Native ioBroker API (`iobroker.quickscript`)

```javascript
import { getAdapter, cron } from 'iobroker.quickscript';

const adapter = getAdapter();

// Read and write states using native ioBroker adapter methods:
const outdoor = await adapter.getForeignStateAsync('0_userdata.0.outsideTemp');
adapter.log.info(`Outdoor temp: ${outdoor?.val}`);

// Listen to state changes:
const onStateChange = async (id, state) => {
    if (id === '0_userdata.0.test_trigger' && state && !state.ack) {
        await adapter.setForeignStateAsync('0_userdata.0.test_response', {
            val: `Handled at ${new Date().toISOString()}`,
            ack: true,
        });
    }
};
adapter.on('stateChange', onStateChange);

// Schedule cron tasks via node-cron:
const job = cron.schedule('*/5 * * * *', async () => {
    await adapter.setForeignStateAsync('zwave2.0.Node_004.Thermostat_Setpoint.setpoint_heating', 21);
});

// Cleanup hook before hot-reload or adapter stop:
adapter.onCleanup(() => {
    adapter.removeListener('stateChange', onStateChange);
    job.stop();
});
```

### Option 2: Compatibility Layer (`iobroker.quickscript/compat.js`)

For quick migration of existing scripts from `iobroker.javascript`:

```javascript
import { getState, setState, on, schedule, sendTo, log, onStop } from 'iobroker.quickscript/compat.js';

on('zigbee.0.a4c138a03abaa4b0.brightness_step_up', (obj) => {
    if (obj.val === true) {
        const cur = getState('zwave2.0.Node_024.Multilevel_Switch.targetValue').val || 0;
        setState('zwave2.0.Node_024.Multilevel_Switch.targetValue', Math.min(99, cur + 5));
    }
});

schedule('0 * * 1-3,12 *', async () => {
    const res = await fetch('https://example.com/data.json');
    const data = await res.json();
    log(`Loaded ${data.length} items`);
});
```

---

## Changelog
<!--
    Placeholder for the next version (at the beginning of the line):
    ### **WORK IN PROGRESS**
-->
### 0.1.0 (2026-10-30)
* (Constantin Bosneaga) initial release

---

## License
MIT License

Copyright (c) 2026 Constantin Bosneaga <c.bosneaga@xsolla.com>

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
