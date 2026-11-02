# AGENTS.md — Writing Automation Scripts for `ioBroker.quickscript`

This directory contains automation scripts executed by **`ioBroker.quickscript`** (native **Rust + QuickJS ES2023** engine, ~2.5 MB RAM).

---

## 1. File & Directory Rules

1. **Root directory (`*.js`):**
   - Every `.js` file in the root directory (not starting with `_` or `.`) is automatically loaded and executed on startup.
2. **Shared libraries (`./lib/*.js` or `_*.js`):**
   - Put shared helpers/configs inside `./lib/` (e.g. `./lib/heating-lib.js`).
   - Subdirectories are **not** auto-executed as entrypoints; they are loaded when imported via `import { ... } from './lib/...'`.
3. **Hot-Reload:**
   - Saving any `.js` file in the root directory or `./lib/` automatically drops the entire QuickJS VM (clearing all old listeners, cron jobs, and timers) and reloads all scripts from disk within 1 second.
   - Do **not** write manual cleanup boilerplate (`removeListener`, etc.) — the VM is recreated cleanly on every reload.

---

## 2. Module Syntax & Available API

Always use **ES Modules (`import` / `export`)**. CommonJS (`require`, `module.exports`) is **not** supported.

```js
import {
    getState,
    setState,
    on,
    schedule,
    sendTo,
    log,
    setTimeout,
    clearTimeout,
    setInterval,
    clearInterval,
} from 'iobroker.quickscript';
```

### API Reference

- **`await getState(id)`**
  - Reads an ioBroker state by full ID (e.g. `'0_userdata.0.outsideTemp'`).
  - Returns `{ val, ack, ts, lc, from }` or `null` if not found.
- **`await setState(id, val, ack = false)`**
  - Writes a state value. Use `ack = false` (default) to command devices (e.g. Z-Wave thermostats, MQTT, Zigbee) and `ack = true` to record status/data states (e.g. `'0_userdata.0.*'`).
- **`on(patternOrOptions, optionsOrCallback, callback?)`**
  - Subscribes to state changes.
  - Examples:
    - `on('zigbee.0.knob.brightness_step_up', (obj) => { ... })` (default: `change: 'ne'`)
    - `on('zwave2.0.Node_004.Setpoint', { change: 'ne', ack: true }, (obj) => { ... })`
    - `on({ id: '0_userdata.0.peakDays', change: 'ne' }, (obj) => { ... })`
    - `on(['id.1', 'id.2'], (obj) => { ... })`
  - Callback receives `obj = { id, state: { val, ack, ts, lc, from }, oldState: { val, ... } }`.
- **`schedule(cronExpr, callback)`**
  - Runs `callback` on a standard 5-field cron schedule: `'min hour dom month dow'`.
  - Supported syntax: `*`, `*/15`, `5`, `1-5`, `1,15,30`.
  - Examples: `schedule('*/15 * * * *', updateHeating)`, `schedule('0 */6 * * *', fetchPeaks)`.
- **`sendTo(adapterInstance, command, message)`**
  - Sends a message to another adapter via ioBroker messagebox.
  - Example: `sendTo('telegram.0', 'send', { text: 'Peak event detected' })`.
- **`log(message, level = 'info')`**
  - Logs to ioBroker Admin UI and `/opt/iobroker/log/iobroker.YYYY-MM-DD.log`.
  - Levels: `'info'`, `'warn'`, `'error'`, `'debug'`.
  - Do **not** prepend timestamps to `message` — ioBroker adds timestamps automatically.
- **`await fetch(url)`**
  - Built-in HTTP/HTTPS GET client provided by Rust (`ureq` + `rustls`).
  - Returns `{ ok, status, async text(), async json() }`.
- **`setTimeout(fn, ms)` / `clearTimeout(id)` / `setInterval(fn, ms)` / `clearInterval(id)`**
  - Available both as globals and as named imports from `'iobroker.quickscript'`.

---

## 3. Engine Constraints (What NOT to use)

Scripts run inside **QuickJS (ES2023)**, **not** Node.js:

1. **No Node.js built-in modules:** Do **not** import `fs`, `path`, `http`, `net`, `child_process`, `crypto`, or `os`.
2. **No `npm` packages:** Do **not** import external npm packages.
3. **No `Intl` locale tables (`toLocaleDateString` / `toLocaleTimeString` options):**
   - QuickJS does not bundle the 30 MB ICU `Intl` dataset.
   - Instead of `now.toLocaleDateString('en', { weekday: 'short' })`, use:
     ```js
     const dayName = ['sun', 'mon', 'tue', 'wed', 'thu', 'fri', 'sat'][now.getDay()];
     ```
4. **Only `fetch(url)` GET requests:** `fetch` is synchronous on the underlying Rust socket with a 15-second timeout and returns a Promise in JS.
