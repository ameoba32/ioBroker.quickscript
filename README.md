# ioBroker.quickscript

![Logo](admin/quickscript.svg)

[![NPM version](https://img.shields.io/npm/v/iobroker.quickscript.svg)](https://www.npmjs.com/package/iobroker.quickscript)
[![Downloads](https://img.shields.io/npm/dm/iobroker.quickscript.svg)](https://www.npmjs.com/package/iobroker.quickscript)
![Number of Installations](https://iobroker.live/badges/quickscript-installed.svg)
![Current version in stable repository](https://iobroker.live/badges/quickscript-stable.svg)

**Ultra-lightweight (~2.5 MB RAM) native automation engine for ioBroker powered by Rust and QuickJS.**

Designed for users, developers, and AI agents who want to manage automation scripts as **plain JavaScript files on disk or in a Git repository**, with instant sub-millisecond hot-reload, without the massive memory overhead of Node.js / V8 or a browser IDE.

---

## Why QuickScript?

| Feature | Standard `iobroker.javascript` | `iobroker.quickscript` |
| :--- | :--- | :--- |
| **Memory Footprint** | **80 MB – 250+ MB** (Node.js / V8 heap) | **~2.5 MB RAM** (Native Rust + QuickJS) |
| **Script Storage** | Stored inside ioBroker Objects DB | **Plain `.js` files on disk / Git repo** |
| **Hot Reload** | Slow, prone to memory leaks / orphan timers | **Instant (~2 ms)** via full VM teardown (`drop`) |
| **Architecture** | Heavy Node.js runtime & admin web-IDE | Native binary (self-replacing via `execv()`) |
| **Protocol** | Heavy adapter-core client | Direct Redis RESP protocol to States & Objects DB |
| **AI Agents & IDE** | Hard to edit outside browser UI | **100% Git, VS Code, AI-agent friendly** |

---

## Key Highlights

* **Ultra-Low Memory (~2.5 MB RSS):** Written in Rust using QuickJS (ES2023). Spawns via Node.js N-API loader and immediately executes `execv()` on itself, dropping Node.js from RAM completely while keeping the same PID for `js-controller`.
* **Direct Redis Protocol (RESP):** Connects straight to ioBroker's internal States DB (`:9000`) and Objects DB (`:9001`) over TCP.
* **Instant Clean Hot-Reload:** Watches your scripts directory. When any file changes, the entire QuickJS runtime and context are cleanly dropped, instantly freeing all memory and removing old timers/listeners, then re-executed in ~2 ms.
* **Built-in Cron & Fetch:** High-performance cron engine in Rust (supporting standard 5-field cron syntax) plus native global `fetch()`.
* **Resilient Connection:** Automatic reconnection and subscription replay if the database or `js-controller` restarts.

---

## Directory Structure

Point the adapter configuration to your scripts directory (or use the default path):

```text
/opt/iobroker/iobroker-data/quickscripts/   <- scripts directory
├── 01_heating.js                          <- automatically executed
├── 02_lighting.js                         <- automatically executed
├── 03_security.js                         <- automatically executed
└── lib/                                   <- subdirectories for shared modules
    └── helpers.js                         <- imported by root scripts, not run standalone
```

* **Root directory (`*.js`):** All top-level JavaScript files are executed concurrently on startup.
* **Subdirectories (`lib/`, etc.):** Files in subfolders are shared modules. Import them in your scripts using standard ES module syntax: `import { helper } from './lib/helpers.js'`.

---

## Usage Example

```javascript
import { getState, setState, on, schedule, sendTo, log, setTimeout, clearTimeout } from 'iobroker.quickscript';

log('Automation initialized');

// 1. React to state changes
on('zigbee.0.sensor.temperature', async (obj) => {
    log(`Temperature changed: ${obj.val}°C`);
    if (obj.val < 18) {
        await setState('heating.0.living_room.target', 21);
    }
});

// 2. Schedule cron jobs (5 fields: minute, hour, day, month, weekday)
schedule('0 7 * * 1-5', async () => {
    log('Weekday morning routine started');
    const res = await fetch('https://api.weather.com/v1/forecast');
    const data = await res.json();
    sendTo('telegram.0', 'send', { text: `Good morning! Forecast: ${data.summary}` });
});
```

---

## Supported API

* `getState(id)`: Returns `{ val, ack, ts, lc, from }`.
* `setState(id, val, ack = false)`: Updates the state in ioBroker.
* `on(id, callback)`: Subscribes to changes of a specific state.
* `schedule(cronString, callback)`: Schedules a task using standard cron syntax.
* `sendTo(targetAdapter, command, message)`: Sends a message to another adapter (e.g., Telegram, Pushover).
* `log(message, level = 'info')`: Logs to the console and ioBroker log viewer.
* `setTimeout`, `clearTimeout`, `setInterval`, `clearInterval`: Standard timer functions.
* `fetch(url, options)`: Standard global HTTP/HTTPS fetch.

---

## Installation

Install the adapter via the ioBroker Admin web interface:
1. Open the **Adapters** tab in ioBroker Admin.
2. Search for **QuickScript** and click the **+** (Add) icon.

---

## Changelog

### 0.2.6 (2026-11-03)
* Translate `common.news` into all languages (pt, nl, fr, it, es, pl, uk, zh-cn) as the repository checker requires.

### 0.2.5 (2026-11-03)
* Add English and German user guide (`docs/en`, `docs/de`).
* Add npm Trusted Publishing deploy job; releases are published from GitHub Actions with provenance.
* Remove unused `api.js` and the `node-cron` dependency: the SDK and the cron engine are built into the Rust binary.

### 0.2.4 (2026-11-02)
* Add winston ANSI color sequences to log file entries for ioBroker.admin LogsWorker parser.

### 0.2.3 (2026-11-02)
* Add automatic Redis States DB reconnection and subscription replay on connection loss.

### 0.2.2 (2026-11-02)
* Hybrid single-binary architecture: unified `.node` N-API loader and Rust executable via `execv()`.
* Memory usage reduced to ~2.5 MB RSS.

### 0.2.0 (2026-11-02)
* Native Rust engine with QuickJS (ES2023) runtime.
* Direct RESP protocol connection to ioBroker States & Objects DB.
* High-performance 5-field cron engine and native `fetch` support.

### 0.1.0 (2026-10-30)
* Initial release (JavaScript / Compact Mode edition).

---

## License
MIT License

Copyright (c) 2026 Constantin Bosneaga <c.bosneaga@gmail.com>

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

