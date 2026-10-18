# RynthV

<p align="center">
  <img src="assets/branding/app-icon.png" alt="RynthV アイコン" width="96" height="96">
</p>

Rust と egui で開発された歌声合成エンジンです。

[English](README.md) · [简体中文](README.zh-CN.md) · [한국어](README.ko.md) · [Русский](README.ru.md)

> **このフォークは macOS 対応を追加しています。** 変更内容は下記の [macOS](#macos) と
> [MACOS-SUPPORT.md](MACOS-SUPPORT.md) を参照してください。
> 上流リポジトリ：[cubeww/rynthv](https://github.com/cubeww/rynthv)。

## ビルドと実行

Rust 1.99.0 が必要です（`rust-toolchain.toml` で固定）。

### Windows

MSVC、Windows SDK、DX12 対応 GPU が必要です。

```powershell
cargo run --release
```

### Linux

X11 / Wayland に対応し、Vulkan または OpenGL を使用します。Ubuntu 24.04 での例：

```sh
sudo apt install build-essential pkg-config libasound2-dev libwayland-dev libxkbcommon-dev \
  fontconfig fonts-noto-cjk libvulkan1 libegl1 libgl1 xdg-desktop-portal xdg-desktop-portal-gtk
cargo build --release --locked --bins
./target/release/rynthv
```

動作にはグラフィックドライバ、ALSA オーディオデバイス、デスクトップの XDG ポータルバックエンド
（KDE では `xdg-desktop-portal-kde`）が必要です。

### macOS

macOS 11 以降と Metal 対応 GPU が必要です。Apple Silicon では arm64 ネイティブでビルドされます。

```sh
cargo build --release --bins
./target/release/rynthv
```

追加のシステムパッケージは不要です。描画は Metal、音声出力は CoreAudio、UI フォントは
システムから読み込みます（PingFang SC、なければ Hiragino Sans GB、さらに STHeiti の順にフォールバック）。

`rust-toolchain.toml` が Rust 1.99.0 を固定しているため、初回ビルド時に `rustup` が自動で導入します。

## 音源ライブラリと設定

初回起動時に、設定画面で音源ライブラリのディレクトリと発音辞書のディレクトリを手動で選択してください。
リソースパスの自動検出は行わず、音源・辞書・ユーザーのモデルコンテキストは同梱していません。

| 音源の種類 | Windows | Linux | macOS |
| --- | --- | --- | --- |
| 第一世代音源 | 対応 | 対応 | 対応 |
| 第二世代音源 | モデルコンテキストの取り込みが必要 | モデルコンテキストの取り込みが必要 | モデルコンテキストの取り込みが必要 |

設定ディレクトリ：

| プラットフォーム | 設定 | モデルコンテキスト |
| --- | --- | --- |
| Windows | `%LOCALAPPDATA%/RynthV` | `%APPDATA%/RynthV/context.json` |
| Linux | `$XDG_CONFIG_HOME/RynthV`（未設定時は `$HOME/.config/RynthV`） | 設定ディレクトリ内の `context.json` |
| macOS | `~/Library/Application Support/RynthV` | 設定ディレクトリ内の `context.json` |

コマンドラインの合成・診断ツールは
`cargo run --release --locked --bin rynthv-synth -- --help` で確認できます。

## macOS でのモデルコンテキスト取り込み

第二世代音源にはモデルコンテキストが必要です。4 つの取得元のうち 3 つが macOS で利用できます。

| 取得元 | Windows | Linux | macOS |
| --- | --- | --- | --- |
| コンテキストファイル | 対応 | 対応 | 対応 |
| 手動入力 | 対応 | 対応 | 対応 |
| ローカルセッションファイル | 対応 | 非対応 | 非対応 |
| オリジナルから取得（オンライン） | 対応 | 非対応 | 非対応 |

後者 2 つは Windows x64 上のオリジナルエディタのセッションファイルやプロセスメモリを読む必要があります。
macOS では UI 上でこの 2 項目が無効化され、コンテキストファイルまたは手動入力を使うよう案内されます。

## プラットフォーム対応一覧

| 機能 | Windows | Linux | macOS |
| --- | --- | --- | --- |
| 編集 UI | DX12 | Vulkan / OpenGL | Metal |
| 音声出力 | WASAPI | ALSA | CoreAudio |
| UI フォント | Microsoft YaHei | fontconfig（`zh-cn`） | PingFang SC / Hiragino Sans GB / STHeiti |
| 主修飾キー | Ctrl | Ctrl | ⌘ Command |
| ネイティブ CPU | x86-64 | x86-64 / arm64 | arm64（Apple Silicon） |

## ライセンス

MIT。このフォークは元の著作権表示を保持しています。詳細は [LICENSE](LICENSE) を参照してください。
