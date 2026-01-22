# Litra Glow

[![CI/CD](https://github.com/3axap4eHko/litra/actions/workflows/build.yml/badge.svg)](https://github.com/3axap4eHko/litra/actions/workflows/build.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![GitHub release](https://img.shields.io/github/v/release/3axap4eHko/litra)](https://github.com/3axap4eHko/litra/releases)

Cross-platform Logitech Litra Glow controller with a native GUI.

![Logitech Litra Glow UI](assets/screenshot.jpg)

## Features

- Control brightness and color temperature
- Headless CLI mode for scripting
- System tray integration
- Auto-reconnect on device plug/unplug
- Native look and feel on Windows, macOS, and Linux
- Centers the window on the monitor under the cursor at startup

## Download

Pre-built binaries are available on the [Releases](https://github.com/3axap4eHko/litra/releases) page:

| Platform | File |
|----------|------|
| Windows x64 | `litra-glow-windows-x86_64.zip` |
| macOS Apple Silicon | `litra-glow-macos-aarch64.tar.gz` |
| macOS Intel | `litra-glow-macos-x86_64.tar.gz` |
| Linux x64 | `litra-glow-linux-x86_64.tar.gz` |

## Building from Source

### Prerequisites (Linux only)

```bash
sudo apt-get install libusb-1.0-0-dev libudev-dev libdbus-1-dev libx11-dev
```

### Build

```bash
cargo build --release
```

The binary will be at `target/release/litra-glow` (or `litra-glow.exe` on Windows).

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

### GUI Mode

Run the application without arguments to launch the GUI:

```bash
./litra-glow
```

### Headless (CLI) Mode

When any CLI flag is provided, the app runs in headless mode and exits after applying the command.
It does not launch the GUI, which makes it suitable for scripts and automation.

Windows note: release builds use the GUI subsystem, so headless output is written to the parent
terminal if one exists. Run the command from PowerShell or CMD to see `--status` output.

Control the lamp directly from the command line:

```bash
# Show current status (JSON output)
./litra-glow --status
# {"power":true,"brightness":50,"temperature":4000}

# Power control
./litra-glow --on
./litra-glow --off
./litra-glow --toggle

# Set brightness (0-100%)
./litra-glow --brightness 50

# Set color temperature (2700-6500K)
./litra-glow --temperature 4000

# Combined commands
./litra-glow --on --brightness 75 --temperature 5000

# Show help
./litra-glow --help
```

### Debug Logging

Enable debug logging:

```bash
RUST_LOG=debug ./litra-glow
```

## License

MIT License - Copyright 2026 Ivan Zakharchanka
