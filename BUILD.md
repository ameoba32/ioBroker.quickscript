# Build

The native engine lives in `rust/`. The adapter ships **prebuilt binaries** in
`bin/`; CI does not compile Rust. Every release needs new binaries committed.

## What the binary is

`rust/src/main.rs` builds a plain executable with a dual entry point:

1. Started directly, it runs the adapter engine.
2. Loaded with `require()` from `main.js`, Node calls
   `napi_register_module_v1`, which `execv()`s the same file and replaces the
   Node process.

`patch_pie.c` clears the `DF_1_PIE` flag so the same file works as both a
plugin and an executable.

## Requirements

- Rust toolchain (`rustup`)
- `gcc` and `make`
- libclang for `bindgen` (rquickjs). Any LLVM 16+ works.
- Internet access for crates.io on the first build

No `apt` and no root needed: a libclang wheel is enough.

```bash
curl -sL -o /tmp/libclang.whl \
  https://files.pythonhosted.org/packages/1d/fc/716c1e62e512ef1c160e7984a73a5fc7df45166f2ff3f254e71c58076f7c/libclang-18.1.1-py2.py3-none-manylinux2010_x86_64.whl
python3 -m zipfile -e /tmp/libclang.whl /tmp/libclang18
export LIBCLANG_PATH=/tmp/libclang18/libclang-18.1.1.data/platlib/clang/native
export BINDGEN_EXTRA_CLANG_ARGS="-isystem /usr/lib/gcc/x86_64-linux-gnu/14/include"
```

`BINDGEN_EXTRA_CLANG_ARGS` is needed when clang cannot find `stddef.h`
(gcc ships it, LLVM does not). Adjust the gcc version to your host.

`rust/.cargo/config.toml` adds `--export-dynamic`. Do not remove it: without it
the binary builds fine but Node refuses to load it with
`Module did not self-register`.

## Build x86_64 (native host)

```bash
cd rust
cargo build --release
gcc patch_pie.c -o /tmp/patch_pie
/tmp/patch_pie target/release/quickscript-rs
cp target/release/quickscript-rs ../bin/quickscript-linux-x64.node
chmod +x ../bin/quickscript-linux-x64.node
```

## Build aarch64 (e.g. on a Raspberry Pi / ARM NAS)

Cross-compiling needs an ARM linker. The simplest reliable way is a native
build in a container:

```bash
docker run --rm -v "$PWD/..:/src" -w /src/rust rust:1-bookworm bash -c "
  apt-get update -qq && apt-get install -y -qq libclang-dev clang >/dev/null &&
  cargo build --release -j1 &&
  gcc patch_pie.c -o /tmp/pp && /tmp/pp target/release/quickscript-rs"
cp target/release/quickscript-rs ../bin/quickscript-linux-arm64.node
chmod +x ../bin/quickscript-linux-arm64.node
```

`-j1` keeps memory under ~1 GB for small ARM boards.

## Verify before committing

The dynamic symbol must be present in both files:

```bash
python3 - <<'PY'
import sys, struct
for path in ("bin/quickscript-linux-arm64.node", "bin/quickscript-linux-x64.node"):
    d = open(path, "rb").read()
    print(path, b"napi_register_module_v1" in d, len(d))
PY
```

Both must print `True`. A `False` means the file will fail with
`Module did not self-register`.

Optional runtime check without starting the adapter (the symbol is only looked
up, never called):

```bash
python3 -c "import ctypes; lib=ctypes.CDLL('bin/quickscript-linux-arm64.node'); print(lib.napi_register_module_v1)"
```

## Release

1. Bump `version` in `package.json` and `io-package.json`.
2. Commit `rust/`, `bin/` and the version files.
3. `git tag vX.Y.Z && git push origin main vX.Y.Z`.
4. CI publishes to npm via OIDC.

## Deploy into a running ioBroker

The container keeps its own copy under
`/opt/iobroker/node_modules/iobroker.quickscript/bin/`. Copy the new file
there, keep a backup, then restart the instance:

```bash
docker exec iobroker-iobroker-1 cp .../bin/quickscript-linux-arm64.node .../bin/quickscript-linux-arm64.node.bak
docker cp bin/quickscript-linux-arm64.node iobroker-iobroker-1:/opt/iobroker/node_modules/iobroker.quickscript/bin/quickscript-linux-arm64.node
docker exec iobroker-iobroker-1 chown iobroker:iobroker .../bin/quickscript-linux-arm64.node
docker exec iobroker-iobroker-1 sh -c "cd /opt/iobroker && iobroker restart quickscript.0"
```

Check the log for `Loaded script:` lines before declaring success.
