# RynthV

<p align="center">
  <img src="assets/branding/app-icon.png" alt="RynthV 图标" width="96" height="96">
</p>

使用 Rust + egui 开发的歌声合成引擎。

[English](README.md) · [日本語](README.ja.md) · [한국어](README.ko.md) · [Русский](README.ru.md)

> **本分支增加了 macOS 支持。** 改动说明见下方 [macOS](#macos) 一节与 [MACOS-SUPPORT.md](MACOS-SUPPORT.md)。
> 上游仓库：[cubeww/rynthv](https://github.com/cubeww/rynthv)。

## 构建与运行

需要 Rust 1.99.0，版本由 `rust-toolchain.toml` 指定。

### Windows

需要 MSVC、Windows SDK 和支持 DX12 的显卡。

```powershell
cargo run --release
```

### Linux

支持 X11 / Wayland，使用 Vulkan 或 OpenGL。Ubuntu 24.04 构建示例：

```sh
sudo apt install build-essential pkg-config libasound2-dev libwayland-dev libxkbcommon-dev \
  fontconfig fonts-noto-cjk libvulkan1 libegl1 libgl1 xdg-desktop-portal xdg-desktop-portal-gtk
cargo build --release --locked --bins
./target/release/rynthv
```

需要可用的图形驱动、ALSA 音频设备和桌面对应的 XDG 门户后端（KDE 使用 `xdg-desktop-portal-kde`）。

### macOS

需要 macOS 11 或更新版本，以及支持 Metal 的显卡。Apple Silicon 上为原生 arm64 构建。

```sh
cargo build --release --bins
./target/release/rynthv
```

无需额外安装系统包：渲染走 Metal，音频走 CoreAudio，界面字体从系统加载
（PingFang SC，缺失时依次退回 Hiragino Sans GB、STHeiti）。

由于 `rust-toolchain.toml` 固定了 Rust 1.99.0，首次构建时 `rustup` 会自动安装该工具链。

## 声库与配置

首次使用请在设置中手动选择声库目录和发音词典目录，程序不会自动探测资源路径，也不附带声库、词典或用户模型上下文。

| 声库类型 | Windows | Linux | macOS |
| --- | --- | --- | --- |
| 一代声库 | 支持 | 支持 | 支持 |
| 二代声库 | 需要导入模型上下文 | 需要导入模型上下文 | 需要导入模型上下文 |

配置目录：

| 平台 | 配置 | 模型上下文 |
| --- | --- | --- |
| Windows | `%LOCALAPPDATA%/RynthV` | `%APPDATA%/RynthV/context.json` |
| Linux | `$XDG_CONFIG_HOME/RynthV`（未设置时为 `$HOME/.config/RynthV`） | 配置目录下的 `context.json` |
| macOS | `~/Library/Application Support/RynthV` | 配置目录下的 `context.json` |

命令行合成与诊断工具的用法可通过 `cargo run --release --locked --bin rynthv-synth -- --help` 查看。

## macOS 上导入模型上下文

二代声库需要模型上下文。四种来源中有三种可在 macOS 上使用：

| 来源 | Windows | Linux | macOS |
| --- | --- | --- | --- |
| 上下文文件 | 支持 | 支持 | 支持 |
| 手动输入 | 支持 | 支持 | 支持 |
| 本地会话文件 | 支持 | 不支持 | 不支持 |
| 从原版获取（在线） | 支持 | 不支持 | 不支持 |

后两种需要读取 Windows x64 上原版编辑器的会话文件或进程内存。macOS 上界面会将这两项置灰，
并提示改用上下文文件或手动输入。

## 平台支持一览

| 功能 | Windows | Linux | macOS |
| --- | --- | --- | --- |
| 编辑界面 | DX12 | Vulkan / OpenGL | Metal |
| 音频输出 | WASAPI | ALSA | CoreAudio |
| 界面字体 | 微软雅黑 | fontconfig（`zh-cn`） | PingFang SC / Hiragino Sans GB / STHeiti |
| 主快捷键修饰键 | Ctrl | Ctrl | ⌘ Command |
| 原生 CPU | x86-64 | x86-64 / arm64 | arm64（Apple Silicon） |

## 许可证

MIT。本分支保留原始版权声明，详见 [LICENSE](LICENSE)。
