# macOS support

This fork makes RynthV build and run on macOS. The upstream project targets Windows and Linux
only; every change below closes a gap that made the macOS build fail or left a feature unusable.

Verified on an Apple M4 (arm64) running macOS, with Rust 1.99.0.

## What was broken

| Area | Symptom | Cause |
| --- | --- | --- |
| Convolution kernel | Build failed at `compile_error!` | `kernel/sse2.rs` required `target_arch = "x86_64"` |
| Rendering | Panic at startup | No wgpu backend feature for macOS |
| Configuration | CLI aborted on every command | `config_root()` had no macOS branch |
| UI font | Chinese text invisible | `system_font()` returned an error on macOS |
| Context import | Always failed | `location::verify()` had no macOS branch |
| Shortcuts | ⌘ did nothing | Every binding used `Modifiers::CTRL` |

## Changes

### 1. Metal backend

`Cargo.toml` gains a macOS-only wgpu feature. Windows keeps `dx12`, Linux keeps `vulkan` + `gles`.

```toml
[target.'cfg(target_os = "macos")'.dependencies]
wgpu = { version = "=30.0.1", default-features = false, features = ["metal"] }
```

`Cargo.lock` only gains `wgpu-core-deps-apple`; no dependency versions change.

### 2. Portable convolution kernel

The packed convolution backend was written directly against SSE2 intrinsics and rejected every
other architecture. `kernel/portable.rs` provides the same four operations in scalar form, and
`kernel.rs` selects it by target:

```rust
#[cfg(target_arch = "x86_64")]
mod sse2;
#[cfg(not(target_arch = "x86_64"))]
#[path = "kernel/portable.rs"]
mod sse2;
```

Both modules are named `sse2`, so no other line in `kernel.rs` changes.

**Numerical equivalence.** The SSE2 path accumulates one output lane at a time, walking inputs in
order: `sum = sum + weight * sample`, with no reassociation and no FMA. The portable path uses the
same order per lane, so results are bit-identical and generated audio does not differ between
architectures. This matters because the module documentation explicitly promises that the
reduction order is preserved.

The SSE2 file is left untouched, so x86-64 builds are unaffected.

### 3. Configuration directory

`config_root()` gains a macOS branch pointing at `~/Library/Application Support`, matching the
platform convention. Windows and Linux branches are unchanged.

### 4. UI font

`ui/theme/fonts/macos.rs` resolves a Simplified Chinese font in this order:

1. **PingFang SC** — the system default. It ships as a downloadable asset, so its path varies
   between macOS releases; the code probes `/System/Library/Fonts`, `/System/Library/Fonts/Supplemental`
   and every `AssetsV2/*/AssetData` directory.
2. **Hiragino Sans GB** — W3 for regular, W6 for bold.
3. **STHeiti Medium / Light** — final fallback.

PingFang is a collection; the Simplified Chinese faces are selected by index (Regular 3,
Medium 7, Semibold 11). Each index was checked to contain both `CFF ` and `cmap` tables, so the
faces are usable OpenType/CFF outlines.

### 5. Context import location check

`imported/location.rs` had Windows and Linux implementations of `verify()` and an error fallback.
On macOS every context read, lock and write went through that fallback, so importing a model
context could never succeed.

The macOS implementation mirrors the Linux one, with two platform differences:

- `O_NOFOLLOW` is `0x100` on macOS, not `0x20000`.
- macOS has no `/proc/self/fd`, and `/dev/fd/N` cannot be read with `read_link`. The open
  descriptor's canonical path is obtained with `fcntl(F_GETPATH)` instead.

`fcntl` is declared through `extern "C"` following the project's existing FFI style, so no new
dependency is introduced.

### 6. Platform shortcuts

Every shortcut was registered with `Modifiers::CTRL`, so ⌘ was dead on macOS. egui maps
`Modifiers::COMMAND` to ⌘ on macOS and to Ctrl everywhere else
(`command = if cfg!(target_os = "macos") { super_ } else { ctrl }`), so switching to it leaves
Windows and Linux behaviour unchanged.

Changed sites: the main shortcut handler in `ui/chrome.rs`, lyric entry confirmation, and the
pitch-anchor select-all, copy, cut, paste and octave-shift bindings.

Menu and tooltip hints follow the same mapping through `shortcut_text()`, which renders
`Ctrl+` / `Shift+` / `Alt+` as `⌘` / `⇧` / `⌥` on macOS and returns the input unchanged elsewhere.

## What still requires Windows

Two context sources read data that only exists on Windows x64:

- **Local session file** — the original editor's saved session.
- **From the original editor (online)** — the running editor's process memory.

These are gated on `all(windows, target_arch = "x86_64")` upstream. The UI already disables both
and directs the user to the context-file or manual-entry sources, which do work on macOS.

## Verification

- `cargo build --release --bins` and `cargo clippy --release --bins` on macOS arm64.
- `cargo check --release --bins` for `x86_64-pc-windows-msvc` and `x86_64-unknown-linux-gnu`,
  with stub resource-compiler and pkg-config files so the checks reach the Rust sources.
- Runtime: the editor launches and the interface renders Chinese text.

Cross-platform checks are compile-only; linking and running on Windows and Linux still need a
native machine or CI.

## Notes for upstream

- `AGENTS.md` asks not to keep backwards-compatibility layers. The portable kernel is selected by
  `cfg`, not by a runtime fallback, and the SSE2 path is unchanged.
- No tests were added, per `AGENTS.md`; verification was done by building and running the editor.
