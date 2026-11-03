# QuickScript — Benutzerhandbuch (Deutsch)

QuickScript führt einfache JavaScript-Dateien aus einem Verzeichnis auf der Festplatte aus.
Die Engine ist in Rust geschrieben und nutzt die QuickJS-Laufzeit. Der Speicherbedarf liegt bei etwa 2,5 MB.

## 1. Installation

1. Öffne den Reiter **Adapter** in der ioBroker-Administration.
2. Suche **QuickScript**.
3. Klicke auf **+**.
4. Erstelle eine Instanz.

Der Adapter benötigt `js-controller >= 6.0.11` und `admin >= 7.6.20`.

## 2. Skriptverzeichnis einstellen

Öffne die Instanzkonfiguration. Setze **scriptsDir**.

Standardverzeichnis:

```text
/opt/iobroker/iobroker-data/quickscripts/
```

## 3. Verzeichnisstruktur

```text
/opt/iobroker/iobroker-data/quickscripts/
├── 01_heating.js      <- wird beim Start ausgeführt
├── 02_lighting.js     <- wird beim Start ausgeführt
└── lib/
    └── helpers.js     <- nur importiert, nie allein ausgeführt
```

1. Jede `.js`-Datei im Hauptverzeichnis wird beim Start ausgeführt.
2. Dateien in Unterverzeichnissen sind gemeinsame Module. Importiere sie.
3. Dateien, die mit `_` oder `.` beginnen, werden ignoriert.
4. Speichere eine Datei, um alle Skripte neu zu laden. Das dauert etwa 1 Sekunde.

## 4. Modulsyntax

Nutze ES-Module. CommonJS (`require`) steht in Skripten nicht zur Verfügung.

```javascript
import { getState, setState, on, schedule, sendTo, log } from 'iobroker.quickscript';
```

## 5. API

- `await getState(id)` — gibt `{ val, ack, ts, lc, from }` oder `null` zurück.
- `await setState(id, val, ack = false)` — schreibt einen State.
- `on(pattern, callback)` — abonniert Zustandsänderungen.
- `schedule(cronExpr, callback)` — führt eine Aufgabe nach einem 5-Feld-Cron-Ausdruck aus.
- `sendTo(instance, command, message, callback)` — sendet eine Nachricht an einen anderen Adapter.
- `log(message, level)` — schreibt ins ioBroker-Log.
- `setTimeout`, `clearTimeout`, `setInterval`, `clearInterval` — Timer, werden beim Neuladen entfernt.
- `await fetch(url)` — GET-Anfrage mit 15 Sekunden Timeout.

## 6. Beispiel

```javascript
import { on, setState, log } from 'iobroker.quickscript';

on('zigbee.0.sensor.temperature', async (obj) => {
    if (obj.val < 18) {
        await setState('heating.0.living_room.target', 21);
        log('Heizung auf 21 gesetzt');
    }
});
```

## 7. Grenzen

1. Keine npm-Pakete.
2. Keine Node.js-Module: `fs`, `path`, `http`, `crypto`, `child_process`, `os`.
3. Keine `Intl`-Sprachtabellen.
4. `fetch` unterstützt nur GET-Anfragen.

## 8. Fehlersuche

- **Skripte starten nicht.** Prüfe `scriptsDir` in der Instanzkonfiguration.
- **Änderungen greifen nicht.** Prüfe den Speicherort der Datei. Lies das Log auf Syntaxfehler.
- **Adapter startet nicht.** Prüfe `info.connection` und das Log. Unterstützte Plattformen: linux-x64, linux-arm64.

## 9. Lizenz

MIT. Copyright (c) 2026 Constantin Bosneaga.
