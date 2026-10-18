# RynthV

<p align="center">
  <img src="assets/branding/app-icon.png" alt="RynthV icon" width="96" height="96">
</p>

A singing voice synthesis engine built with Rust and egui.

[简体中文](README.zh-CN.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · [Русский](README.ru.md)

> **This fork adds macOS support.** See [macOS](#macos) below and
> [MACOS-SUPPORT.md](MACOS-SUPPORT.md) for what changed and why.
> Upstream: [cubeww/rynthv](https://github.com/cubeww/rynthv).

## Building and running

Requires Rust 1.99.0, pinned by `rust-toolchain.toml`.

### Windows

Needs MSVC, the Windows SDK and a GPU with DX12 support.

```powershell
cargo run --release
```

### Linux

Supports X11 and Wayland with Vulkan or OpenGL. Build example for Ubuntu 24.04:

```sh
sudo apt install build-essential pkg-config libasound2-dev libwayland-dev libxkbcommon-dev \
  fontconfig fonts-noto-cjk libvulkan1 libegl1 libgl1 xdg-desktop-portal xdg-desktop-portal-gtk
cargo build --release --locked --bins
./target/release/rynthv
```

Requires a working graphics driver, an ALSA audio device and a desktop XDG portal backend
(`xdg-desktop-portal-kde` on KDE).

### macOS

Needs macOS 11 or newer and a GPU with Metal support. On Apple Silicon the build is native arm64.

```sh
cargo build --release --bins
./target/release/rynthv
```

No extra system packages are required: Metal is the rendering backend, CoreAudio provides audio
output, and the UI font is loaded from the system (PingFang SC, falling back to Hiragino Sans GB
and then STHeiti).

Because the project pins Rust 1.99.0 in `rust-toolchain.toml`, `rustup` installs that toolchain
automatically on first build.

## Voice libraries and configuration

On first launch, select the voice library directory and the pronunciation dictionary directory
in Settings. The program never probes resource paths automatically and ships without voice
libraries, dictionaries or user model contexts.

| Voice type | Windows | Linux | macOS |
| --- | --- | --- | --- |
| First-generation voices | Supported | Supported | Supported |
| Second-generation voices | Model context required | Model context required | Model context required |

Configuration directories:

| Platform | Configuration | Model context |
| --- | --- | --- |
| Windows | `%LOCALAPPDATA%/RynthV` | `%APPDATA%/RynthV/context.json` |
| Linux | `$XDG_CONFIG_HOME/RynthV` (or `$HOME/.config/RynthV`) | `context.json` in the configuration directory |
| macOS | `~/Library/Application Support/RynthV` | `context.json` in the configuration directory |

The command-line synthesis and diagnostic tools are documented under
`cargo run --release --locked --bin rynthv-synth -- --help`.

## Importing a model context on macOS

Second-generation voices need a model context. Three of the four sources work on macOS:

| Source | Windows | Linux | macOS |
| --- | --- | --- | --- |
| Context file | Yes | Yes | Yes |
| Manual entry | Yes | Yes | Yes |
| Local session file | Yes | No | No |
| From the original editor (online) | Yes | No | No |

The last two read the original editor's session file or process memory, which only exists on
Windows x64. On macOS the UI disables those two sources and points at the context-file and
manual-entry paths instead.

## Platform support at a glance

| Feature | Windows | Linux | macOS |
| --- | --- | --- | --- |
| Editor UI | DX12 | Vulkan / OpenGL | Metal |
| Audio output | WASAPI | ALSA | CoreAudio |
| UI font | Microsoft YaHei | fontconfig (`zh-cn`) | PingFang SC / Hiragino Sans GB / STHeiti |
| Primary shortcut modifier | Ctrl | Ctrl | ⌘ Command |
| Native CPU | x86-64 | x86-64 / arm64 | arm64 (Apple Silicon) |

## License

MIT. This fork keeps the original copyright notice; see [LICENSE](LICENSE).
