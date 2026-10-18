# RynthV

<p align="center">
  <img src="assets/branding/app-icon.png" alt="RynthV 아이콘" width="96" height="96">
</p>

Rust와 egui로 개발된 노래 합성 엔진입니다.

[English](README.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md) · [Русский](README.ru.md)

> **이 포크는 macOS 지원을 추가합니다.** 변경 내용은 아래 [macOS](#macos) 항목과
> [MACOS-SUPPORT.md](MACOS-SUPPORT.md)를 참고하세요.
> 업스트림 저장소: [cubeww/rynthv](https://github.com/cubeww/rynthv).

## 빌드 및 실행

Rust 1.99.0이 필요하며 `rust-toolchain.toml`에 고정되어 있습니다.

### Windows

MSVC, Windows SDK, DX12 지원 GPU가 필요합니다.

```powershell
cargo run --release
```

### Linux

X11 / Wayland를 지원하며 Vulkan 또는 OpenGL을 사용합니다. Ubuntu 24.04 예시:

```sh
sudo apt install build-essential pkg-config libasound2-dev libwayland-dev libxkbcommon-dev \
  fontconfig fonts-noto-cjk libvulkan1 libegl1 libgl1 xdg-desktop-portal xdg-desktop-portal-gtk
cargo build --release --locked --bins
./target/release/rynthv
```

그래픽 드라이버, ALSA 오디오 장치, 데스크톱 XDG 포털 백엔드(KDE는 `xdg-desktop-portal-kde`)가 필요합니다.

### macOS

macOS 11 이상과 Metal 지원 GPU가 필요합니다. Apple Silicon에서는 arm64 네이티브로 빌드됩니다.

```sh
cargo build --release --bins
./target/release/rynthv
```

추가 시스템 패키지는 필요 없습니다. 렌더링은 Metal, 오디오 출력은 CoreAudio를 사용하며,
UI 폰트는 시스템에서 읽어옵니다(PingFang SC, 없으면 Hiragino Sans GB, 그다음 STHeiti 순으로 대체).

`rust-toolchain.toml`이 Rust 1.99.0을 고정하므로 첫 빌드 시 `rustup`이 자동으로 설치합니다.

## 보이스 라이브러리와 설정

처음 실행할 때 설정에서 보이스 라이브러리 디렉터리와 발음 사전 디렉터리를 직접 선택하세요.
프로그램은 리소스 경로를 자동으로 탐색하지 않으며, 보이스·사전·사용자 모델 컨텍스트를 포함하지 않습니다.

| 보이스 종류 | Windows | Linux | macOS |
| --- | --- | --- | --- |
| 1세대 보이스 | 지원 | 지원 | 지원 |
| 2세대 보이스 | 모델 컨텍스트 가져오기 필요 | 모델 컨텍스트 가져오기 필요 | 모델 컨텍스트 가져오기 필요 |

설정 디렉터리:

| 플랫폼 | 설정 | 모델 컨텍스트 |
| --- | --- | --- |
| Windows | `%LOCALAPPDATA%/RynthV` | `%APPDATA%/RynthV/context.json` |
| Linux | `$XDG_CONFIG_HOME/RynthV` (미설정 시 `$HOME/.config/RynthV`) | 설정 디렉터리의 `context.json` |
| macOS | `~/Library/Application Support/RynthV` | 설정 디렉터리의 `context.json` |

명령줄 합성·진단 도구는
`cargo run --release --locked --bin rynthv-synth -- --help`로 확인할 수 있습니다.

## macOS에서 모델 컨텍스트 가져오기

2세대 보이스에는 모델 컨텍스트가 필요합니다. 네 가지 소스 중 세 가지가 macOS에서 동작합니다.

| 소스 | Windows | Linux | macOS |
| --- | --- | --- | --- |
| 컨텍스트 파일 | 지원 | 지원 | 지원 |
| 수동 입력 | 지원 | 지원 | 지원 |
| 로컬 세션 파일 | 지원 | 미지원 | 미지원 |
| 원본에서 가져오기(온라인) | 지원 | 미지원 | 미지원 |

마지막 두 가지는 Windows x64의 원본 편집기 세션 파일이나 프로세스 메모리를 읽어야 합니다.
macOS에서는 UI에서 이 두 항목이 비활성화되며 컨텍스트 파일이나 수동 입력을 사용하도록 안내합니다.

## 플랫폼 지원 요약

| 기능 | Windows | Linux | macOS |
| --- | --- | --- | --- |
| 편집 UI | DX12 | Vulkan / OpenGL | Metal |
| 오디오 출력 | WASAPI | ALSA | CoreAudio |
| UI 폰트 | Microsoft YaHei | fontconfig(`zh-cn`) | PingFang SC / Hiragino Sans GB / STHeiti |
| 기본 보조 키 | Ctrl | Ctrl | ⌘ Command |
| 네이티브 CPU | x86-64 | x86-64 / arm64 | arm64(Apple Silicon) |

## 라이선스

MIT. 이 포크는 원본 저작권 표시를 유지합니다. 자세한 내용은 [LICENSE](LICENSE)를 참고하세요.
