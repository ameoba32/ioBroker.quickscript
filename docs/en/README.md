# QuickScript — User Guide (English)

QuickScript runs plain JavaScript files from a directory on disk. The engine is written in Rust
with the QuickJS runtime. It uses about 2.5 MB of RAM.

## 1. Install

1. Open the **Adapters** tab in ioBroker Admin.
2. Search for **QuickScript**.
3. Click the **+** button.
4. Create one instance.

The adapter needs `js-controller >= 6.0.11` and `admin >= 7.6.20`.

## 2. Configure the scripts directory

Open the instance configuration. Set **scriptsDir**.

The default directory is:

```text
/opt/iobroker/iobroker-data/quickscripts/
```

Use one directory per ioBroker instance. The adapter watches this directory.

## 3. Directory layout

```text
/opt/iobroker/iobroker-data/quickscripts/
├── 01_heating.js      <- executed on start
├── 02_lighting.js     <- executed on start
└── lib/
    └── helpers.js     <- imported only, never executed alone
```

1. Every `.js` file in the root directory is executed on start.
2. Files in subdirectories are shared modules. Import them.
3. Files that start with `_` or `.` are ignored in the root directory.
4. Save a file to reload all scripts. The adapter rebuilds the runtime in about 1 second.

## 4. Module syntax

Use ES modules. CommonJS (`require`) is not available inside scripts.

```javascript
import { getState, setState, on, schedule, sendTo, log } from 'iobroker.quickscript';
```

## 5. API

- `await getState(id)` — returns `{ val, ack, ts, lc, from }` or `null`.
- `await setState(id, val, ack = false)` — writes a state.
- `on(pattern, callback)` — subscribes to state changes. Accepts an object: `{ id, change, ack }`.
- `schedule(cronExpr, callback)` — runs a task on a standard 5-field cron expression.
- `sendTo(instance, command, message, callback)` — sends a message to another adapter.
- `log(message, level)` — writes to the ioBroker log. Levels: `info`, `warn`, `error`, `debug`.
- `setTimeout`, `clearTimeout`, `setInterval`, `clearInterval` — timers, removed on reload.
- `await fetch(url)` — GET request. Timeout is 15 seconds.

## 6. Example

```javascript
import { getState, setState, on, schedule, log } from 'iobroker.quickscript';

on('zigbee.0.sensor.temperature', async (obj) => {
    if (obj.val < 18) {
        await setState('heating.0.living_room.target', 21);
        log('Heating set to 21');
    }
});

schedule('0 7 * * 1-5', () => log('Morning routine started'));
```

## 7. Limits

1. No npm packages. The runtime has no `node_modules`.
2. No Node.js modules: `fs`, `path`, `http`, `crypto`, `child_process`, `os`.
3. No `Intl` locale tables. Build day names and number formats yourself.
4. `fetch` supports GET requests only.

## 8. Troubleshooting

- **Scripts do not start.** Check `scriptsDir` in the instance configuration.
- **Changes do not apply.** Check that the file is in the root directory or in `lib/`. Check the adapter log for a syntax error.
- **Adapter does not start.** Check `info.connection`. Read the log for the binary error. Supported platforms: linux-x64, linux-arm64.

## 9. Changelog

See the changelog section in [README.md](../../README.md#changelog).

## 10. License

MIT. Copyright (c) 2026 Constantin Bosneaga.
