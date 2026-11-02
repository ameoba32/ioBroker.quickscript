use chrono::{Datelike, Local, Timelike};
use rquickjs::{
    loader::{Loader, Resolver},
    Context, Ctx, Error as QjsError, Function, Module, Object, Result as QjsResult, Runtime, Value,
};
use serde_json::{json, Value as JsonValue};
use std::{
    collections::{HashMap, HashSet},
    env, fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpStream,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    time::{Duration, Instant, SystemTime},
};

const STATES_PREFIX: &str = "io.";
const OBJECTS_PREFIX: &str = "cfg.o.";
const MESSAGE_PREFIX: &str = "messagebox.";
const LOG_PREFIX: &str = "log.";
const STATUS_EXPIRE_SECONDS: u64 = 25;

// ============================================================================
// Dual-mode entry point: when loaded via Node.js `require()`, Node calls
// `napi_register_module_v1`, which immediately `execv`s this same binary,
// replacing the 140 MB Node.js process in-place under the exact same PID!
// ============================================================================

#[no_mangle]
pub unsafe extern "C" fn napi_register_module_v1(
    _env: *mut std::ffi::c_void,
    _exports: *mut std::ffi::c_void,
) -> *mut std::ffi::c_void {
    let mut info: libc::Dl_info = std::mem::zeroed();
    if libc::dladdr(napi_register_module_v1 as *const std::ffi::c_void, &mut info) != 0
        && !info.dli_fname.is_null()
    {
        let inst = env::var("IOB_INSTANCE").unwrap_or_else(|_| "0".to_string());
        if let Ok(c_inst) = std::ffi::CString::new(inst) {
            let _ = std::io::stdout().flush();
            let _ = std::io::stderr().flush();
            let args = [info.dli_fname, c_inst.as_ptr(), std::ptr::null()];
            libc::execv(info.dli_fname, args.as_ptr());
        }
    }
    std::ptr::null_mut()
}

// ============================================================================
// Minimal, zero-overhead Redis RESP2 client for ioBroker's lowercase protocol
// ============================================================================

struct RespConn {
    host: String,
    port: u16,
    read_timeout: Option<Duration>,
    reader: BufReader<TcpStream>,
    writer: TcpStream,
}

impl RespConn {
    fn connect(host: &str, port: u16, read_timeout: Option<Duration>) -> Result<Self, String> {
        let addr = format!("{}:{}", host, port);
        let stream = TcpStream::connect(&addr)
            .map_err(|e| format!("Failed to connect to {}: {}", addr, e))?;
        let _ = stream.set_nodelay(true);
        let _ = stream.set_read_timeout(read_timeout);
        let writer = stream
            .try_clone()
            .map_err(|e| format!("Failed to clone stream: {}", e))?;
        Ok(Self {
            host: host.to_string(),
            port,
            read_timeout,
            reader: BufReader::new(stream),
            writer,
        })
    }

    fn reconnect(&mut self) -> bool {
        if let Ok(new_conn) = Self::connect(&self.host, self.port, self.read_timeout) {
            self.reader = new_conn.reader;
            self.writer = new_conn.writer;
            true
        } else {
            false
        }
    }

    fn send_cmd(&mut self, args: &[&str]) -> Result<(), String> {
        let mut buf = format!("*{}\r\n", args.len()).into_bytes();
        for arg in args {
            let bytes = arg.as_bytes();
            buf.extend_from_slice(format!("${}\r\n", bytes.len()).as_bytes());
            buf.extend_from_slice(bytes);
            buf.extend_from_slice(b"\r\n");
        }
        self.writer
            .write_all(&buf)
            .map_err(|e| format!("Write error: {}", e))?;
        self.writer
            .flush()
            .map_err(|e| format!("Flush error: {}", e))
    }

    fn read_resp(&mut self) -> Result<RespValue, String> {
        let mut line = String::new();
        let n = match self.reader.read_line(&mut line) {
            Ok(n) => n,
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                return Err("TIMEOUT".to_string());
            }
            Err(e) => return Err(format!("Read error: {}", e)),
        };
        if n == 0 {
            return Err("EOF".to_string());
        }
        let line = line.trim_end_matches(&['\r', '\n'][..]);
        if line.is_empty() {
            return Ok(RespValue::Nil);
        }
        let prefix = line.as_bytes()[0] as char;
        let rest = &line[1..];
        match prefix {
            '+' => Ok(RespValue::Simple(rest.to_string())),
            '-' => Err(format!("Redis error: {}", rest)),
            ':' => Ok(RespValue::Int(rest.parse().unwrap_or(0))),
            '$' => {
                let len: isize = rest.parse().unwrap_or(-1);
                if len < 0 {
                    return Ok(RespValue::Nil);
                }
                let mut buf = vec![0u8; len as usize];
                self.reader
                    .read_exact(&mut buf)
                    .map_err(|e| format!("Bulk read error: {}", e))?;
                let mut crlf = [0u8; 2];
                let _ = self.reader.read_exact(&mut crlf);
                Ok(RespValue::Bulk(String::from_utf8_lossy(&buf).to_string()))
            }
            '*' => {
                let count: isize = rest.parse().unwrap_or(-1);
                if count < 0 {
                    return Ok(RespValue::Nil);
                }
                let mut items = Vec::with_capacity(count as usize);
                for _ in 0..count {
                    items.push(self.read_resp()?);
                }
                Ok(RespValue::Array(items))
            }
            _ => Err(format!("Unexpected RESP prefix: {}", line)),
        }
    }

    fn cmd(&mut self, args: &[&str]) -> Result<RespValue, String> {
        if self.send_cmd(args).is_ok() {
            if let Ok(res) = self.read_resp() {
                return Ok(res);
            }
        }
        if self.reconnect() {
            self.send_cmd(args)?;
            return self.read_resp();
        }
        Err("Connection lost and reconnect failed".to_string())
    }

    fn get_str(&mut self, key: &str) -> Option<String> {
        match self.cmd(&["get", key]).ok()? {
            RespValue::Bulk(s) => Some(s),
            RespValue::Simple(s) => Some(s),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
enum RespValue {
    Simple(String),
    #[allow(dead_code)]
    Int(i64),
    Bulk(String),
    Array(Vec<RespValue>),
    Nil,
}

impl RespValue {
    fn as_str(&self) -> Option<&str> {
        match self {
            RespValue::Simple(s) | RespValue::Bulk(s) => Some(s),
            _ => None,
        }
    }
}

// ============================================================================
// Cron expression matcher (standard 5-field cron: min hour dom month dow)
// ============================================================================

fn match_cron_field(field: &str, value: u32, is_dow: bool) -> bool {
    let field = field.trim();
    if field == "*" {
        return true;
    }
    for part in field.split(',') {
        let part = part.trim();
        if let Some(step_str) = part.strip_prefix("*/") {
            if let Ok(step) = step_str.parse::<u32>() {
                if step > 0 && value % step == 0 {
                    return true;
                }
            }
        } else if part.contains('-') {
            let bounds: Vec<&str> = part.split('-').collect();
            if bounds.len() == 2 {
                if let (Ok(start), Ok(end)) = (bounds[0].parse::<u32>(), bounds[1].parse::<u32>()) {
                    if value >= start && value <= end {
                        return true;
                    }
                }
            }
        } else if let Ok(mut target) = part.parse::<u32>() {
            if is_dow && target == 7 {
                target = 0;
            }
            if value == target {
                return true;
            }
        }
    }
    false
}

fn cron_matches_now(expr: &str, now: &chrono::DateTime<Local>) -> bool {
    let fields: Vec<&str> = expr.split_whitespace().collect();
    // If 6-field cron (with seconds), skip first field if it matches 0
    let slice = if fields.len() == 6 {
        &fields[1..]
    } else if fields.len() == 5 {
        &fields[..]
    } else {
        return false;
    };

    let min = now.minute();
    let hour = now.hour();
    let dom = now.day();
    let month = now.month();
    let dow = now.weekday().num_days_from_sunday();

    match_cron_field(let_slice(slice, 0), min, false)
        && match_cron_field(let_slice(slice, 1), hour, false)
        && match_cron_field(let_slice(slice, 2), dom, false)
        && match_cron_field(let_slice(slice, 3), month, false)
        && match_cron_field(let_slice(slice, 4), dow, true)
}

fn let_slice<'a>(s: &[&'a str], idx: usize) -> &'a str {
    s.get(idx).copied().unwrap_or("*")
}

// ============================================================================
// Glob matching for state subscriptions
// ============================================================================

fn glob_match(pattern: &str, text: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if !pattern.contains('*') && !pattern.contains('?') {
        return pattern == text;
    }
    let pat_chars: Vec<char> = pattern.chars().collect();
    let txt_chars: Vec<char> = text.chars().collect();
    let (mut p, mut t) = (0usize, 0usize);
    let (mut star_idx, mut match_idx) = (None, 0usize);

    while t < txt_chars.len() {
        if p < pat_chars.len() && (pat_chars[p] == '?' || pat_chars[p] == txt_chars[t]) {
            p += 1;
            t += 1;
        } else if p < pat_chars.len() && pat_chars[p] == '*' {
            star_idx = Some(p);
            match_idx = t;
            p += 1;
        } else if let Some(sp) = star_idx {
            p = sp + 1;
            match_idx += 1;
            t = match_idx;
        } else {
            return false;
        }
    }
    while p < pat_chars.len() && pat_chars[p] == '*' {
        p += 1;
    }
    p == pat_chars.len()
}

// ============================================================================
// Shared Adapter State
// ============================================================================

#[derive(Clone)]
struct SubEntry {
    cb_id: u32,
    patterns: Vec<String>,
    change: String,
    ack: Option<bool>,
}

#[derive(Clone)]
struct CronEntry {
    cb_id: u32,
    expr: String,
}

#[derive(Clone)]
struct TimerEntry {
    id: u32,
    cb_id: u32,
    fire_at: Instant,
    interval_ms: Option<u64>,
}

struct SharedState {
    namespace: String,
    instance_id: String,
    states_conn: RespConn,
    log_targets: HashSet<String>,
    log_seq: u64,
    last_states: HashMap<String, JsonValue>,
    subs: Vec<SubEntry>,
    crons: Vec<CronEntry>,
    timers: Vec<TimerEntry>,
    next_timer_id: u32,
    new_psub_patterns: Vec<String>,
}

impl SharedState {
    fn log(&mut self, level: &str, msg: &str) {
        let now = Local::now();
        let now_str = now.format("%Y-%m-%d %H:%M:%S%.3f");
        let pid = std::process::id();
        let formatted_line = format!("{}  - {}: {} ({}) {}\n", now_str, level, self.namespace, pid, msg);
        print!("{}", formatted_line);
        let _ = std::io::stdout().flush();

        let log_dir = Path::new("/opt/iobroker/log");
        if log_dir.is_dir() {
            let log_file = log_dir.join(format!("iobroker.{}.log", now.format("%Y-%m-%d")));
            if let Ok(mut f) = fs::OpenOptions::new().create(true).append(true).open(log_file) {
                let _ = f.write_all(formatted_line.as_bytes());
            }
        }

        if self.log_targets.is_empty() {
            return;
        }
        self.log_seq += 1;
        let ts = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        let payload = json!({
            "_id": self.log_seq,
            "from": self.namespace,
            "ts": ts,
            "severity": level,
            "message": format!("{} ({}) {}", self.namespace, pid, msg),
        })
        .to_string();

        let targets: Vec<String> = self.log_targets.iter().cloned().collect();
        for target in targets {
            let chan = format!("{}{}", LOG_PREFIX, target);
            let _ = self.states_conn.cmd(&["publish", &chan, &payload]);
        }
    }

    fn get_foreign_state(&mut self, id: &str) -> String {
        let key = format!("{}{}", STATES_PREFIX, id);
        let raw = self.states_conn.get_str(&key);
        if let Some(s) = raw {
            if let Ok(v) = serde_json::from_str::<JsonValue>(&s) {
                self.last_states.insert(id.to_string(), v);
            }
            s
        } else {
            "null".to_string()
        }
    }

    fn set_foreign_state(&mut self, id: &str, val_json: &str, ack: bool, expire_sec: Option<u64>) {
        let key = format!("{}{}", STATES_PREFIX, id);
        let ts = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let parsed_val: JsonValue = serde_json::from_str(val_json).unwrap_or(JsonValue::Null);
        let mut lc = ts;
        if let Some(prev_str) = self.states_conn.get_str(&key) {
            if let Ok(prev_obj) = serde_json::from_str::<JsonValue>(&prev_str) {
                if prev_obj.get("val") == Some(&parsed_val) {
                    if let Some(prev_lc) = prev_obj.get("lc").and_then(|x| x.as_u64()) {
                        lc = prev_lc;
                    }
                }
            }
        }

        let wire = json!({
            "val": parsed_val,
            "ack": ack,
            "ts": ts,
            "q": 0,
            "from": self.instance_id,
            "lc": lc,
        })
        .to_string();

        if let Some(exp) = expire_sec {
            let exp_str = exp.to_string();
            let _ = self.states_conn.cmd(&["setex", &key, &exp_str, &wire]);
        } else {
            let _ = self.states_conn.cmd(&["set", &key, &wire]);
        }
        let _ = self.states_conn.cmd(&["publish", &key, &wire]);
    }

    fn send_to(&mut self, target: &str, command: &str, msg_json: &str) {
        let msg_val: JsonValue = serde_json::from_str(msg_json).unwrap_or(JsonValue::Null);
        let full_target = if target.starts_with("system.adapter.") {
            target.to_string()
        } else {
            format!("system.adapter.{}", target)
        };
        let chan = format!("{}{}", MESSAGE_PREFIX, full_target);
        let payload = json!({
            "command": command,
            "message": msg_val,
            "from": self.instance_id,
        })
        .to_string();
        let _ = self.states_conn.cmd(&["publish", &chan, &payload]);
    }
}

// ============================================================================
// Embedded JS API module ('iobroker.quickscript' / 'iobroker.quickscript/api.js')
// ============================================================================

const BUILTIN_API_JS: &str = r#"
export async function getState(id) {
    const raw = globalThis.__qs_get_state(String(id));
    return raw && raw !== 'null' ? JSON.parse(raw) : null;
}

export async function setState(id, val, ack = false) {
    let actualVal = val;
    let actualAck = Boolean(ack);
    if (val !== null && typeof val === 'object' && !Array.isArray(val) && 'val' in val) {
        actualVal = val.val;
        if ('ack' in val) actualAck = Boolean(val.ack);
    }
    globalThis.__qs_set_state(String(id), JSON.stringify(actualVal), actualAck);
}

export function on(patternOrOpts, optionsOrCb, maybeCb) {
    let cb = typeof optionsOrCb === 'function' ? optionsOrCb : maybeCb;
    if (typeof cb !== 'function') return;

    let idPattern = patternOrOpts;
    let change = 'ne';
    let ack = undefined;

    if (typeof patternOrOpts === 'object' && !Array.isArray(patternOrOpts) && patternOrOpts !== null) {
        idPattern = patternOrOpts.id;
        if (patternOrOpts.change) change = patternOrOpts.change;
        if (patternOrOpts.ack !== undefined) ack = Boolean(patternOrOpts.ack);
    } else if (typeof optionsOrCb === 'object' && optionsOrCb !== null) {
        if (optionsOrCb.change) change = optionsOrCb.change;
        if (optionsOrCb.ack !== undefined) ack = Boolean(optionsOrCb.ack);
    }

    const patterns = Array.isArray(idPattern) ? idPattern.map(String) : [String(idPattern)];
    const cbId = globalThis.__qs_register_cb(cb);
    globalThis.__qs_on(
        cbId,
        JSON.stringify(patterns),
        String(change),
        ack === undefined ? '' : String(ack)
    );
}

export function schedule(expr, cb) {
    if (typeof cb !== 'function') return;
    const cbId = globalThis.__qs_register_cb(cb);
    globalThis.__qs_schedule(cbId, String(expr));
}

export function sendTo(adapterName, command, message) {
    let cmd = command;
    let msg = message;
    if (message === undefined && typeof command !== 'string') {
        msg = command;
        cmd = 'send';
    }
    globalThis.__qs_send_to(String(adapterName), String(cmd || 'send'), JSON.stringify(msg ?? null));
}

export function log(msg, level = 'info') {
    globalThis.__qs_log(String(level || 'info'), String(msg));
}

export function setTimeout(cb, ms = 0) {
    if (typeof cb !== 'function') return 0;
    const cbId = globalThis.__qs_register_cb(cb);
    return globalThis.__qs_set_timer(cbId, Number(ms) || 0, false);
}

export function clearTimeout(timerId) {
    if (timerId) globalThis.__qs_clear_timer(Number(timerId));
}

export function setInterval(cb, ms = 0) {
    if (typeof cb !== 'function') return 0;
    const cbId = globalThis.__qs_register_cb(cb);
    return globalThis.__qs_set_timer(cbId, Number(ms) || 0, true);
}

export function clearInterval(timerId) {
    if (timerId) globalThis.__qs_clear_timer(Number(timerId));
}

export default {
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
};
"#;

const BOOTSTRAP_GLOBALS_JS: &str = r#"
globalThis.__qs_callbacks = new Map();
globalThis.__qs_next_cb_id = 1;
globalThis.__qs_register_cb = function(fn) {
    const id = globalThis.__qs_next_cb_id++;
    globalThis.__qs_callbacks.set(id, fn);
    return id;
};
globalThis.__qs_invoke_cb = function(id, argJson) {
    const fn = globalThis.__qs_callbacks.get(id);
    if (!fn) return;
    const arg = argJson ? JSON.parse(argJson) : undefined;
    Promise.resolve().then(() => fn(arg)).catch(err => {
        globalThis.__qs_log('error', 'Callback error: ' + (err && err.stack ? err.stack : err));
    });
};

globalThis.console = {
    log: (...a) => globalThis.__qs_log('info', a.map(String).join(' ')),
    info: (...a) => globalThis.__qs_log('info', a.map(String).join(' ')),
    warn: (...a) => globalThis.__qs_log('warn', a.map(String).join(' ')),
    error: (...a) => globalThis.__qs_log('error', a.map(String).join(' ')),
    debug: (...a) => globalThis.__qs_log('debug', a.map(String).join(' ')),
};

globalThis.fetch = async function(url) {
    const raw = globalThis.__qs_fetch(String(url));
    const res = JSON.parse(raw);
    if (res.error) {
        throw new Error(res.error);
    }
    return {
        ok: res.status >= 200 && res.status < 300,
        status: res.status,
        async text() { return res.body; },
        async json() { return JSON.parse(res.body); },
    };
};

globalThis.setTimeout = function(cb, ms = 0) {
    if (typeof cb !== 'function') return 0;
    const cbId = globalThis.__qs_register_cb(cb);
    return globalThis.__qs_set_timer(cbId, Number(ms) || 0, false);
};
globalThis.clearTimeout = function(id) {
    if (id) globalThis.__qs_clear_timer(Number(id));
};
globalThis.setInterval = function(cb, ms = 0) {
    if (typeof cb !== 'function') return 0;
    const cbId = globalThis.__qs_register_cb(cb);
    return globalThis.__qs_set_timer(cbId, Number(ms) || 0, true);
};
globalThis.clearInterval = function(id) {
    if (id) globalThis.__qs_clear_timer(Number(id));
};
"#;

// ============================================================================
// QuickJS Engine wrapper — dropped and recreated cleanly on every hot-reload
// ============================================================================

// ============================================================================
// Custom Module Resolver & Loader for 'iobroker.quickscript' and relative files
// ============================================================================

struct QuickScriptResolver {
    scripts_dir: PathBuf,
}

impl Resolver for QuickScriptResolver {
    fn resolve<'js>(&mut self, _ctx: &Ctx<'js>, base: &str, name: &str) -> QjsResult<String> {
        if name == "iobroker.quickscript"
            || name == "iobroker.quickscript/api.js"
            || name == "iobroker.quickscript/compat.js"
        {
            return Ok("iobroker.quickscript".to_string());
        }

        let base_dir = if base.is_empty() || base == "iobroker.quickscript" {
            self.scripts_dir.clone()
        } else {
            let bp = Path::new(base);
            if bp.is_absolute() {
                bp.parent().unwrap_or(&self.scripts_dir).to_path_buf()
            } else {
                self.scripts_dir
                    .join(bp)
                    .parent()
                    .unwrap_or(&self.scripts_dir)
                    .to_path_buf()
            }
        };

        let mut candidate = base_dir.join(name);
        if !candidate.exists() && candidate.extension().is_none() {
            candidate.set_extension("js");
        }
        if let Ok(canon) = fs::canonicalize(&candidate) {
            return Ok(canon.to_string_lossy().to_string());
        }
        if candidate.exists() {
            return Ok(candidate.to_string_lossy().to_string());
        }
        Err(QjsError::new_resolving(base, name))
    }
}

struct QuickScriptLoader;

impl Loader for QuickScriptLoader {
    fn load<'js>(&mut self, ctx: &Ctx<'js>, name: &str) -> QjsResult<Module<'js>> {
        if name == "iobroker.quickscript" {
            return Module::declare(ctx.clone(), name, BUILTIN_API_JS);
        }
        let source = fs::read_to_string(name).map_err(|_| QjsError::new_loading(name))?;
        Module::declare(ctx.clone(), name, source)
    }
}

struct JsEngine {
    rt: Runtime,
    ctx: Context,
}

impl JsEngine {
    fn new_and_load(scripts_dir: &Path, shared: Arc<Mutex<SharedState>>) -> Result<(Self, usize), String> {
        {
            let mut st = shared.lock().unwrap();
            st.subs.clear();
            st.crons.clear();
            st.timers.clear();
        }

        let rt = Runtime::new().map_err(|e| format!("Failed to create QuickJS runtime: {}", e))?;

        rt.set_loader(
            QuickScriptResolver {
                scripts_dir: scripts_dir.to_path_buf(),
            },
            QuickScriptLoader,
        );

        let ctx = Context::full(&rt).map_err(|e| format!("Failed to create QuickJS context: {}", e))?;

        ctx.with(|ctx| -> Result<(), String> {
            let globals = ctx.globals();

            let s1 = shared.clone();
            globals
                .set(
                    "__qs_log",
                    Function::new(ctx.clone(), move |level: String, msg: String| {
                        s1.lock().unwrap().log(&level, &msg);
                    })
                    .map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;

            let s2 = shared.clone();
            globals
                .set(
                    "__qs_get_state",
                    Function::new(ctx.clone(), move |id: String| -> String {
                        s2.lock().unwrap().get_foreign_state(&id)
                    })
                    .map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;

            let s3 = shared.clone();
            globals
                .set(
                    "__qs_set_state",
                    Function::new(ctx.clone(), move |id: String, val_json: String, ack: bool| {
                        s3.lock().unwrap().set_foreign_state(&id, &val_json, ack, None);
                    })
                    .map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;

            let s4 = shared.clone();
            globals
                .set(
                    "__qs_send_to",
                    Function::new(
                        ctx.clone(),
                        move |target: String, cmd: String, msg_json: String| {
                            s4.lock().unwrap().send_to(&target, &cmd, &msg_json);
                        },
                    )
                    .map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;

            let s5 = shared.clone();
            globals
                .set(
                    "__qs_on",
                    Function::new(
                        ctx.clone(),
                        move |cb_id: u32, patterns_json: String, change: String, ack_str: String| {
                            let patterns: Vec<String> =
                                serde_json::from_str(&patterns_json).unwrap_or_default();
                            let ack = match ack_str.as_str() {
                                "true" => Some(true),
                                "false" => Some(false),
                                _ => None,
                            };
                            let mut st = s5.lock().unwrap();
                            for pat in &patterns {
                                st.new_psub_patterns.push(format!("{}{}", STATES_PREFIX, pat));
                            }
                            st.subs.push(SubEntry {
                                cb_id,
                                patterns,
                                change,
                                ack,
                            });
                        },
                    )
                    .map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;

            let s6 = shared.clone();
            globals
                .set(
                    "__qs_schedule",
                    Function::new(ctx.clone(), move |cb_id: u32, expr: String| {
                        s6.lock().unwrap().crons.push(CronEntry { cb_id, expr });
                    })
                    .map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;

            let s7 = shared.clone();
            globals
                .set(
                    "__qs_set_timer",
                    Function::new(ctx.clone(), move |cb_id: u32, ms: u64, repeat: bool| -> u32 {
                        let mut st = s7.lock().unwrap();
                        let id = st.next_timer_id;
                        st.next_timer_id += 1;
                        let dur = Duration::from_millis(ms.max(1));
                        st.timers.push(TimerEntry {
                            id,
                            cb_id,
                            fire_at: Instant::now() + dur,
                            interval_ms: if repeat { Some(ms.max(1)) } else { None },
                        });
                        id
                    })
                    .map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;

            let s8 = shared.clone();
            globals
                .set(
                    "__qs_clear_timer",
                    Function::new(ctx.clone(), move |timer_id: u32| {
                        s8.lock().unwrap().timers.retain(|t| t.id != timer_id);
                    })
                    .map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;

            globals
                .set(
                    "__qs_fetch",
                    Function::new(ctx.clone(), move |url: String| -> String {
                        match ureq::get(&url).timeout(Duration::from_secs(15)).call() {
                            Ok(resp) => {
                                let status = resp.status();
                                let body = resp.into_string().unwrap_or_default();
                                json!({ "status": status, "body": body }).to_string()
                            }
                            Err(ureq::Error::Status(code, resp)) => {
                                let body = resp.into_string().unwrap_or_default();
                                json!({ "status": code, "body": body }).to_string()
                            }
                            Err(e) => json!({ "error": e.to_string() }).to_string(),
                        }
                    })
                    .map_err(|e| e.to_string())?,
                )
                .map_err(|e| e.to_string())?;

            ctx.eval::<(), _>(BOOTSTRAP_GLOBALS_JS)
                .map_err(|e| format!("Bootstrap eval error: {}", e))?;

            Ok(())
        })?;

        let mut loaded = 0usize;
        let mut entries: Vec<PathBuf> = fs::read_dir(scripts_dir)
            .map_err(|e| format!("Cannot read scriptsDir {:?}: {}", scripts_dir, e))?
            .filter_map(|e| e.ok().map(|x| x.path()))
            .filter(|p| {
                p.is_file()
                    && p.extension().and_then(|s| s.to_str()) == Some("js")
                    && !p
                        .file_name()
                        .and_then(|s| s.to_str())
                        .unwrap_or("")
                        .starts_with('_')
            })
            .collect();
        entries.sort();

        for path in entries {
            let file_name = path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("script.js")
                .to_string();
            let code = match fs::read_to_string(&path) {
                Ok(c) => c,
                Err(e) => {
                    shared
                        .lock()
                        .unwrap()
                        .log("error", &format!("Failed to read {}: {}", file_name, e));
                    continue;
                }
            };

            ctx.with(|ctx| {
                match Module::evaluate(ctx.clone(), file_name.as_str(), code) {
                    Ok(promise) => {
                        let _ = promise.finish::<Value>();
                        loaded += 1;
                        shared
                            .lock()
                            .unwrap()
                            .log("info", &format!("Loaded script: {}", file_name));
                    }
                    Err(e) => {
                        let err_detail = ctx.catch();
                        shared.lock().unwrap().log(
                            "error",
                            &format!("Error in {}: {} ({:?})", file_name, e, err_detail),
                        );
                    }
                }
            });
            while rt.is_job_pending() {
                let _ = rt.execute_pending_job();
            }
        }

        Ok((Self { rt, ctx }, loaded))
    }

    fn invoke_callback(&self, cb_id: u32, arg_json: Option<String>) {
        self.ctx.with(|ctx| {
            let globals: Object = ctx.globals();
            if let Ok(invoke_fn) = globals.get::<_, Function>("__qs_invoke_cb") {
                let _ = invoke_fn.call::<_, ()>((cb_id, arg_json.unwrap_or_default()));
            }
        });
        while self.rt.is_job_pending() {
            let _ = self.rt.execute_pending_job();
        }
    }
}

// ============================================================================
// Directory modification fingerprint for instant, zero-leak hot-reload
// ============================================================================

fn compute_dir_fingerprint(dir: &Path) -> u64 {
    let mut acc: u64 = 0xcbf29ce484222325;
    fn walk(d: &Path, acc: &mut u64) {
        if let Ok(rd) = fs::read_dir(d) {
            let mut paths: Vec<PathBuf> = rd.filter_map(|e| e.ok().map(|x| x.path())).collect();
            paths.sort();
            for p in paths {
                let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
                if name.starts_with('.') || name == "node_modules" {
                    continue;
                }
                if p.is_dir() {
                    walk(&p, acc);
                } else if p.extension().and_then(|s| s.to_str()) == Some("js") {
                    if let Ok(meta) = fs::metadata(&p) {
                        let mtime = meta
                            .modified()
                            .ok()
                            .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
                            .map(|d| d.as_nanos() as u64)
                            .unwrap_or(0);
                        let len = meta.len();
                        for b in p.to_string_lossy().as_bytes() {
                            *acc ^= *b as u64;
                            *acc = acc.wrapping_mul(0x100000001b3);
                        }
                        *acc ^= mtime ^ len;
                        *acc = acc.wrapping_mul(0x100000001b3);
                    }
                }
            }
        }
    }
    walk(dir, &mut acc);
    acc
}

fn read_rss_mb() -> f64 {
    if let Ok(statm) = fs::read_to_string("/proc/self/statm") {
        if let Some(pages_str) = statm.split_whitespace().nth(1) {
            if let Ok(pages) = pages_str.parse::<u64>() {
                let bytes = pages * 4096;
                return ((bytes as f64 / (1024.0 * 1024.0)) * 100.0).round() / 100.0;
            }
        }
    }
    0.0
}

fn find_iobroker_config() -> (String, u16, String, u16, PathBuf) {
    let candidates = [
        "/opt/iobroker/iobroker-data/iobroker.json",
        "./iobroker-data/iobroker.json",
        "../iobroker-data/iobroker.json",
        "../../iobroker-data/iobroker.json",
    ];
    for c in candidates {
        let p = Path::new(c);
        if p.exists() {
            if let Ok(txt) = fs::read_to_string(p) {
                if let Ok(j) = serde_json::from_str::<JsonValue>(&txt) {
                    let s_host = j["states"]["host"]
                        .as_str()
                        .unwrap_or("127.0.0.1")
                        .to_string();
                    let s_port = j["states"]["port"].as_u64().unwrap_or(9000) as u16;
                    let o_host = j["objects"]["host"]
                        .as_str()
                        .unwrap_or("127.0.0.1")
                        .to_string();
                    let o_port = j["objects"]["port"].as_u64().unwrap_or(9001) as u16;
                    let data_dir = p.parent().unwrap_or(Path::new("/opt/iobroker/iobroker-data")).to_path_buf();
                    return (
                        if s_host == "0.0.0.0" || s_host == "::" { "127.0.0.1".to_string() } else { s_host },
                        s_port,
                        if o_host == "0.0.0.0" || o_host == "::" { "127.0.0.1".to_string() } else { o_host },
                        o_port,
                        data_dir,
                    );
                }
            }
        }
    }
    (
        env::var("IOB_STATES_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
        env::var("IOB_STATES_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(9000),
        env::var("IOB_OBJECTS_HOST").unwrap_or_else(|_| "127.0.0.1".to_string()),
        env::var("IOB_OBJECTS_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(9001),
        PathBuf::from("/opt/iobroker/iobroker-data"),
    )
}

fn main() {
    let instance = env::var("IOB_INSTANCE")
        .ok()
        .or_else(|| env::args().nth(1))
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(0);

    let namespace = format!("quickscript.{}", instance);
    let instance_id = format!("system.adapter.{}", namespace);

    let (s_host, s_port, o_host, o_port, data_dir) = find_iobroker_config();

    let mut objects_conn = RespConn::connect(&o_host, o_port, Some(Duration::from_secs(5)))
        .expect("Failed to connect to ioBroker Objects DB");
    let states_conn = RespConn::connect(&s_host, s_port, Some(Duration::from_secs(5)))
        .expect("Failed to connect to ioBroker States DB");
    let mut sub_conn = RespConn::connect(&s_host, s_port, Some(Duration::from_millis(50)))
        .expect("Failed to connect to ioBroker States PubSub");

    // Read adapter configuration from Objects DB
    let mut scripts_dir = env::var("QUICKSCRIPT_DIR")
        .ok()
        .map(PathBuf::from)
        .unwrap_or_else(|| data_dir.join("quickscripts"));

    if env::var("QUICKSCRIPT_DIR").is_err() {
        let obj_key = format!("{}{}", OBJECTS_PREFIX, instance_id);
        if let Some(raw_obj) = objects_conn.get_str(&obj_key) {
            if let Ok(obj_json) = serde_json::from_str::<JsonValue>(&raw_obj) {
                if let Some(dir_str) = obj_json["native"]["scriptsDir"].as_str() {
                    if !dir_str.trim().is_empty() {
                        scripts_dir = PathBuf::from(dir_str.trim());
                    }
                }
            }
        }
    }

    let _ = fs::create_dir_all(&scripts_dir);

    let shared = Arc::new(Mutex::new(SharedState {
        namespace: namespace.clone(),
        instance_id: instance_id.clone(),
        states_conn,
        log_targets: HashSet::new(),
        log_seq: 0,
        last_states: HashMap::new(),
        subs: Vec::new(),
        crons: Vec::new(),
        timers: Vec::new(),
        next_timer_id: 1,
        new_psub_patterns: Vec::new(),
    }));

    // Discover active log targets (e.g., system.adapter.admin.0.logging)
    {
        let mut st = shared.lock().unwrap();
        if let Ok(RespValue::Array(keys)) = st.states_conn.cmd(&["keys", "io.system.adapter.*.logging"]) {
            for k in keys {
                if let Some(key_str) = k.as_str() {
                    if let Some(val_str) = st.states_conn.get_str(key_str) {
                        if let Ok(v) = serde_json::from_str::<JsonValue>(&val_str) {
                            if v["val"] == json!(true) || v["val"] == json!("true") {
                                let id_part = key_str.trim_start_matches(STATES_PREFIX);
                                let target = id_part.trim_end_matches(".logging").to_string();
                                if target != instance_id {
                                    st.log_targets.insert(target);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Subscribe to internal channels: sigKill & *.logging
    let sigkill_chan = format!("{}{}.sigKill", STATES_PREFIX, instance_id);
    let _ = sub_conn.send_cmd(&["psubscribe", &sigkill_chan, "io.system.adapter.*.logging"]);

    // Report initial alive & connected states
    let start_instant = Instant::now();
    {
        let mut st = shared.lock().unwrap();
        st.set_foreign_state(&format!("{}.alive", instance_id), "true", true, Some(STATUS_EXPIRE_SECONDS));
        st.set_foreign_state(&format!("{}.connected", instance_id), "true", true, Some(STATUS_EXPIRE_SECONDS));
        st.set_foreign_state(&format!("{}.compactMode", instance_id), "false", true, None);
        st.set_foreign_state(&format!("{}.memRss", instance_id), &read_rss_mb().to_string(), true, Some(STATUS_EXPIRE_SECONDS));
        st.set_foreign_state(&format!("{}.info.connection", namespace), "true", true, None);
        st.log(
            "info",
            &format!(
                "QuickScript Rust+QuickJS engine started (PID {}, RAM {:.2} MB). Watching: {:?}",
                std::process::id(),
                read_rss_mb(),
                scripts_dir
            ),
        );
    }

    let (initial_engine, loaded_count) = match JsEngine::new_and_load(&scripts_dir, shared.clone()) {
        Ok(res) => res,
        Err(e) => {
            shared.lock().unwrap().log("error", &format!("Engine load failed: {}", e));
            std::process::exit(1);
        }
    };
    let mut engine: Option<JsEngine> = Some(initial_engine);

    {
        let mut st = shared.lock().unwrap();
        st.set_foreign_state(&format!("{}.info.scriptsLoaded", namespace), &loaded_count.to_string(), true, None);
        let new_pats: Vec<String> = st.new_psub_patterns.drain(..).collect();
        for pat in new_pats {
            let _ = sub_conn.send_cmd(&["psubscribe", &pat]);
        }
    }

    let mut last_fingerprint = compute_dir_fingerprint(&scripts_dir);
    let mut last_fs_check = Instant::now();
    let mut last_heartbeat = Instant::now();
    let mut last_cron_minute: Option<(u32, u32, u32)> = None;

    loop {
        // 1. Drain incoming Redis pub/sub messages (auto-reconnect on EOF / socket error)
        loop {
            match sub_conn.read_resp() {
                Ok(RespValue::Array(items)) => {
                    if items.len() >= 4 && items[0].as_str() == Some("pmessage") {
                        let channel = items[2].as_str().unwrap_or("");
                        let payload = items[3].as_str().unwrap_or("");

                        if channel == sigkill_chan {
                            if let Ok(v) = serde_json::from_str::<JsonValue>(payload) {
                                let kill_val = v["val"].as_i64().unwrap_or(0);
                                if kill_val == -1 || (kill_val > 0 && kill_val as u32 != std::process::id()) {
                                    let mut st = shared.lock().unwrap();
                                    st.log("info", "sigKill received, shutting down cleanly");
                                    st.set_foreign_state(&format!("{}.info.connection", namespace), "false", true, None);
                                    st.set_foreign_state(&format!("{}.alive", instance_id), "false", true, None);
                                    st.set_foreign_state(&format!("{}.connected", instance_id), "false", true, None);
                                    std::process::exit(0);
                                }
                            }
                            continue;
                        }

                        if channel.starts_with("io.system.adapter.") && channel.ends_with(".logging") {
                            let id_part = channel.trim_start_matches(STATES_PREFIX);
                            let target = id_part.trim_end_matches(".logging").to_string();
                            if target != instance_id {
                                if let Ok(v) = serde_json::from_str::<JsonValue>(payload) {
                                    let mut st = shared.lock().unwrap();
                                    if v["val"] == json!(true) || v["val"] == json!("true") {
                                        st.log_targets.insert(target);
                                    } else {
                                        st.log_targets.remove(&target);
                                    }
                                }
                            }
                            continue;
                        }

                        if let Some(state_id) = channel.strip_prefix(STATES_PREFIX) {
                            if let Ok(new_state) = serde_json::from_str::<JsonValue>(payload) {
                                let mut callbacks_to_fire: Vec<(u32, String)> = Vec::new();
                                {
                                    let mut st = shared.lock().unwrap();
                                    let old_state = st
                                        .last_states
                                        .insert(state_id.to_string(), new_state.clone())
                                        .unwrap_or(JsonValue::Null);

                                    for sub in &st.subs {
                                        if !sub.patterns.iter().any(|p| glob_match(p, state_id)) {
                                            continue;
                                        }
                                        if let Some(req_ack) = sub.ack {
                                            if new_state["ack"].as_bool() != Some(req_ack) {
                                                continue;
                                            }
                                        }
                                        if sub.change == "ne" && !old_state.is_null() && old_state["val"] == new_state["val"] {
                                            continue;
                                        }
                                        if sub.change == "gt" && !(new_state["val"].as_f64() > old_state["val"].as_f64()) {
                                            continue;
                                        }
                                        if sub.change == "lt" && !(new_state["val"].as_f64() < old_state["val"].as_f64()) {
                                            continue;
                                        }
                                        let event_json = json!({
                                            "id": state_id,
                                            "state": new_state,
                                            "oldState": old_state,
                                        })
                                        .to_string();
                                        callbacks_to_fire.push((sub.cb_id, event_json));
                                    }
                                }
                                if let Some(ref eng) = engine {
                                    for (cb_id, ev) in callbacks_to_fire {
                                        eng.invoke_callback(cb_id, Some(ev));
                                    }
                                }
                            }
                        }
                    }
                }
                Ok(_) => {}
                Err(ref e) if e == "TIMEOUT" => break,
                Err(_) => {
                    // PubSub socket disconnected — reconnect and replay all subscriptions
                    if sub_conn.reconnect() {
                        let _ = sub_conn.send_cmd(&["psubscribe", &sigkill_chan, "io.system.adapter.*.logging"]);
                        let pats: Vec<String> = {
                            let st = shared.lock().unwrap();
                            st.subs
                                .iter()
                                .flat_map(|s| s.patterns.iter().map(|p| format!("{}{}", STATES_PREFIX, p)))
                                .collect()
                        };
                        for pat in pats {
                            let _ = sub_conn.send_cmd(&["psubscribe", &pat]);
                        }
                        shared.lock().unwrap().log("info", "Reconnected to States PubSub and restored subscriptions");
                    } else {
                        std::thread::sleep(Duration::from_millis(500));
                    }
                    break;
                }
            }
        }

        // 2. Fire due timers (setTimeout / setInterval)
        let now_inst = Instant::now();
        let mut due_timers: Vec<u32> = Vec::new();
        {
            let mut st = shared.lock().unwrap();
            let mut i = 0;
            while i < st.timers.len() {
                if now_inst >= st.timers[i].fire_at {
                    let cb_id = st.timers[i].cb_id;
                    due_timers.push(cb_id);
                    if let Some(interval_ms) = st.timers[i].interval_ms {
                        st.timers[i].fire_at = now_inst + Duration::from_millis(interval_ms);
                        i += 1;
                    } else {
                        st.timers.remove(i);
                    }
                } else {
                    i += 1;
                }
            }
        }
        if let Some(ref eng) = engine {
            for cb_id in due_timers {
                eng.invoke_callback(cb_id, None);
            }
        }

        // 3. Fire due cron schedules once per minute
        let local_now = Local::now();
        let cur_min_key = (local_now.day(), local_now.hour(), local_now.minute());
        if last_cron_minute != Some(cur_min_key) {
            last_cron_minute = Some(cur_min_key);
            let mut due_crons: Vec<u32> = Vec::new();
            {
                let st = shared.lock().unwrap();
                for c in &st.crons {
                    if cron_matches_now(&c.expr, &local_now) {
                        due_crons.push(c.cb_id);
                    }
                }
            }
            if let Some(ref eng) = engine {
                for cb_id in due_crons {
                    eng.invoke_callback(cb_id, None);
                }
            }
        }

        // 4. Check for file changes every 1 second -> full VM drop & recreate
        if now_inst.duration_since(last_fs_check) >= Duration::from_secs(1) {
            last_fs_check = now_inst;
            let fp = compute_dir_fingerprint(&scripts_dir);
            if fp != last_fingerprint {
                last_fingerprint = fp;
                shared
                    .lock()
                    .unwrap()
                    .log("info", "File change detected — dropping old QuickJS VM and reloading scripts...");
                engine = None;
                match JsEngine::new_and_load(&scripts_dir, shared.clone()) {
                    Ok((new_eng, count)) => {
                        engine = Some(new_eng);
                        let mut st = shared.lock().unwrap();
                        st.set_foreign_state(
                            &format!("{}.info.scriptsLoaded", namespace),
                            &count.to_string(),
                            true,
                            None,
                        );
                        let new_pats: Vec<String> = st.new_psub_patterns.drain(..).collect();
                        for pat in new_pats {
                            let _ = sub_conn.send_cmd(&["psubscribe", &pat]);
                        }
                    }
                    Err(e) => {
                        shared
                            .lock()
                            .unwrap()
                            .log("error", &format!("Reload failed: {}", e));
                    }
                }
            }
        }

        // 5. Heartbeat every 15 seconds
        if now_inst.duration_since(last_heartbeat) >= Duration::from_secs(15) {
            last_heartbeat = now_inst;
            let uptime = start_instant.elapsed().as_secs();
            let rss = read_rss_mb();
            let mut st = shared.lock().unwrap();
            st.set_foreign_state(&format!("{}.alive", instance_id), "true", true, Some(STATUS_EXPIRE_SECONDS));
            st.set_foreign_state(&format!("{}.connected", instance_id), "true", true, Some(STATUS_EXPIRE_SECONDS));
            st.set_foreign_state(&format!("{}.uptime", instance_id), &uptime.to_string(), true, Some(STATUS_EXPIRE_SECONDS));
            st.set_foreign_state(&format!("{}.memRss", instance_id), &rss.to_string(), true, Some(STATUS_EXPIRE_SECONDS));
        }
    }
}
