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
* **Selective State Subscriptions:** Subscribes only to the specific state IDs used in your `on(...)` calls instead of loading the entire database into memory.
* **Automatic Hot-Reload (`fs.watch`):** Watches both root scripts and library subfolders (e.g. `./lib/`). On any file change, automatically cleans up previous listeners, cron jobs, and timers, then reloads scripts and modified libraries with fresh cache timestamps.

---

## Folder Structure

* **Root directory (`scriptsDir`):** Only top-level `*.js` files in the root folder are executed automatically as automation scripts.
* **Subdirectories (e.g. `lib/`):** Files in subdirectories are not executed on their own — import them from your root scripts using standard ES `import { ... } from './lib/my-helper.js'`.

---

## Usage Example

```javascript
import { getState, setState, on, schedule, sendTo, log, setTimeout, clearTimeout } from 'iobroker.quickscript';

log('Automation script started');

// 1. Listen to state changes (automatically subscribes and cleans up on hot-reload):
on('zigbee.0.a4c138a03abaa4b0.brightness_step_up', async (obj) => {
    if (obj.val === true) {
        const cur = (await getState('zwave2.0.Node_024.Multilevel_Switch.targetValue')).val || 0;
        await setState('zwave2.0.Node_024.Multilevel_Switch.targetValue', Math.min(99, Number(cur) + 5));
    }
});

// 2. Schedule cron tasks via node-cron (automatically stopped on hot-reload):
schedule('0 * * 1-3,12 *', async () => {
    const res = await fetch('https://example.com/data.json');
    const data = await res.json();
    sendTo('telegram.0', 'send', { text: `Loaded ${data.length} items` });
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
