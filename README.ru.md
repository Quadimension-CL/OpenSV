# RynthV

<p align="center">
  <img src="assets/branding/app-icon.png" alt="Иконка RynthV" width="96" height="96">
</p>

Движок синтеза поющего голоса на Rust и egui.

[English](README.md) · [简体中文](README.zh-CN.md) · [日本語](README.ja.md) · [한국어](README.ko.md)

> **В этом форке добавлена поддержка macOS.** Изменения описаны в разделе [macOS](#macos)
> и в файле [MACOS-SUPPORT.md](MACOS-SUPPORT.md).
> Оригинальный репозиторий: [cubeww/rynthv](https://github.com/cubeww/rynthv).

## Сборка и запуск

Требуется Rust 1.99.0; версия закреплена в `rust-toolchain.toml`.

### Windows

Нужны MSVC, Windows SDK и видеокарта с поддержкой DX12.

```powershell
cargo run --release
```

### Linux

Поддерживаются X11 и Wayland с Vulkan или OpenGL. Пример сборки для Ubuntu 24.04:

```sh
sudo apt install build-essential pkg-config libasound2-dev libwayland-dev libxkbcommon-dev \
  fontconfig fonts-noto-cjk libvulkan1 libegl1 libgl1 xdg-desktop-portal xdg-desktop-portal-gtk
cargo build --release --locked --bins
./target/release/rynthv
```

Нужны рабочий графический драйвер, аудиоустройство ALSA и серверная часть XDG-портала
(в KDE — `xdg-desktop-portal-kde`).

### macOS

Требуются macOS 11 или новее и видеокарта с поддержкой Metal. На Apple Silicon сборка нативная, arm64.

```sh
cargo build --release --bins
./target/release/rynthv
```

Дополнительные системные пакеты не нужны: рендеринг идёт через Metal, вывод звука — через CoreAudio,
а шрифт интерфейса загружается из системы (PingFang SC, затем Hiragino Sans GB, затем STHeiti).

Поскольку `rust-toolchain.toml` закрепляет Rust 1.99.0, `rustup` установит этот инструментарий
автоматически при первой сборке.

## Голосовые библиотеки и настройки

При первом запуске выберите в настройках каталог голосовой библиотеки и каталог словаря произношения.
Программа не ищет ресурсы автоматически и не поставляется с голосами, словарями или контекстами моделей.

| Тип голоса | Windows | Linux | macOS |
| --- | --- | --- | --- |
| Голоса первого поколения | Поддерживаются | Поддерживаются | Поддерживаются |
| Голоса второго поколения | Нужен контекст модели | Нужен контекст модели | Нужен контекст модели |

Каталоги:

| Платформа | Настройки | Контекст модели |
| --- | --- | --- |
| Windows | `%LOCALAPPDATA%/RynthV` | `%APPDATA%/RynthV/context.json` |
| Linux | `$XDG_CONFIG_HOME/RynthV` (или `$HOME/.config/RynthV`) | `context.json` в каталоге настроек |
| macOS | `~/Library/Application Support/RynthV` | `context.json` в каталоге настроек |

Документация по инструментам командной строки:
`cargo run --release --locked --bin rynthv-synth -- --help`.

## Импорт контекста модели в macOS

Голосам второго поколения нужен контекст модели. Три из четырёх источников работают в macOS:

| Источник | Windows | Linux | macOS |
| --- | --- | --- | --- |
| Файл контекста | Да | Да | Да |
| Ручной ввод | Да | Да | Да |
| Локальный файл сессии | Да | Нет | Нет |
| Из оригинального редактора (онлайн) | Да | Нет | Нет |

Последние два читают файл сессии или память процесса оригинального редактора, которые существуют
только в Windows x64. В macOS эти пункты отключены в интерфейсе, а пользователю предлагается
файл контекста или ручной ввод.

## Поддержка платформ

| Возможность | Windows | Linux | macOS |
| --- | --- | --- | --- |
| Интерфейс | DX12 | Vulkan / OpenGL | Metal |
| Вывод звука | WASAPI | ALSA | CoreAudio |
| Шрифт интерфейса | Microsoft YaHei | fontconfig (`zh-cn`) | PingFang SC / Hiragino Sans GB / STHeiti |
| Основной модификатор | Ctrl | Ctrl | ⌘ Command |
| Собственная архитектура | x86-64 | x86-64 / arm64 | arm64 (Apple Silicon) |

## Лицензия

MIT. Форк сохраняет исходное уведомление об авторских правах; см. [LICENSE](LICENSE).
