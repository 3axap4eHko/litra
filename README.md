# Litra

[![CI/CD](https://github.com/3axap4eHko/litra/actions/workflows/build.yml/badge.svg)](https://github.com/3axap4eHko/litra/actions/workflows/build.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![GitHub release](https://img.shields.io/github/v/release/3axap4eHko/litra)](https://github.com/3axap4eHko/litra/releases)

Cross-platform Logitech Litra Glow controller with a native GUI.

![Logitech Litra Glow UI](assets/screenshot.jpg)

## Features

- Control brightness and color temperature
- System tray integration
- Remembers last settings
- Auto-reconnect on device plug/unplug
- Native look and feel on Windows, macOS, and Linux
- Centers the window on the monitor under the cursor at startup

## Download

Pre-built binaries are available on the [Releases](https://github.com/3axap4eHko/litra/releases) page:

| Platform | File |
|----------|------|
| Windows x64 | `litra-windows-x86_64.zip` |
| macOS Apple Silicon | `litra-macos-aarch64.tar.gz` |
| macOS Intel | `litra-macos-x86_64.tar.gz` |
| Linux x64 | `litra-linux-x86_64.tar.gz` |

## Building from Source

### Prerequisites (Linux only)

```bash
sudo apt-get install libusb-1.0-0-dev libudev-dev libdbus-1-dev libx11-dev
```

### Build

```bash
cargo build --release
```

The binary will be at `target/release/litra` (or `litra.exe` on Windows).

## Setup

### Linux

Create a udev rule to allow access without root:

```bash
echo 'SUBSYSTEM=="usb", ATTR{idVendor}=="046d", ATTR{idProduct}=="c900", MODE="0666"' | sudo tee /etc/udev/rules.d/50-litra-glow.rules
sudo udevadm control --reload-rules
sudo udevadm trigger
```

### Windows / macOS

No additional setup required.

### Linux notes

- The app centers on the monitor under the cursor using an X11 cursor query. On Wayland, global cursor position may be blocked, so it falls back to the current or primary monitor.
- On WSLg, some window managers report oversized frame bounds; the app clamps the position to keep the window on-screen.

## Usage

Run the application:

```bash
./litra
```

Enable debug logging:

```bash
RUST_LOG=debug ./litra
```

## License

MIT License - Copyright 2026 Ivan Zakharchanka
