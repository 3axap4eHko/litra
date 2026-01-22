#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod protocol;
mod usb;

use std::cell::Cell;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
use std::rc::Rc;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use clap::Parser;
use device_query::{DeviceQuery, DeviceState as DeviceQueryState};
use log::{debug, error, info, warn};
use protocol::{
    Command, MAX_BRIGHTNESS, MAX_TEMPERATURE, MIN_BRIGHTNESS, MIN_TEMPERATURE, Response,
    TEMPERATURE_STEP,
};
use slint::winit_030::{WinitWindowAccessor, winit};
use usb::LitraDevice;

#[cfg(feature = "tray")]
use std::sync::mpsc as std_mpsc;

slint::include_modules!();

#[derive(Parser)]
#[command(name = "litra-glow", version, about = "Logitech Litra Glow controller")]
struct Cli {
    #[arg(long, help = "Turn the lamp on")]
    on: bool,

    #[arg(long, help = "Turn the lamp off")]
    off: bool,

    #[arg(long, help = "Toggle lamp power")]
    toggle: bool,

    #[arg(long, value_name = "0-100", help = "Set brightness (percentage)")]
    brightness: Option<u8>,

    #[arg(
        long,
        value_name = "KELVIN",
        help = "Set color temperature (2700-6500)"
    )]
    temperature: Option<u16>,

    #[arg(long, help = "Show current lamp status")]
    status: bool,
}

impl Cli {
    fn has_commands(&self) -> bool {
        self.on
            || self.off
            || self.toggle
            || self.brightness.is_some()
            || self.temperature.is_some()
            || self.status
    }
}

#[derive(Debug)]
enum DeviceCommand {
    Retry,
    SetPower(bool),
    SetBrightness(u16),
    SetTemperature(u16),
}

#[derive(Debug)]
enum DeviceEvent {
    Connected,
    Power(bool),
    Brightness(u16),
    Temperature(u16),
    Error(String),
}

const PENDING_TIMEOUT: Duration = Duration::from_millis(300);
const CENTER_RETRY_DELAY: Duration = Duration::from_millis(16);
const CENTER_RETRY_LIMIT: u8 = 15;

#[derive(Debug, Clone, Copy)]
struct DeviceState {
    power: bool,
    brightness: u16,
    temperature: u16,
    pending_brightness: Option<Instant>,
    pending_temperature: Option<Instant>,
}

fn clamp_brightness(value: f32) -> u16 {
    let value = value.round() as i32;
    let clamped = value.clamp(MIN_BRIGHTNESS as i32, MAX_BRIGHTNESS as i32);
    clamped as u16
}

fn clamp_temperature(value: f32) -> u16 {
    let value = value.round() as i32;
    let clamped = value.clamp(MIN_TEMPERATURE as i32, MAX_TEMPERATURE as i32) as u16;
    let step = TEMPERATURE_STEP;
    let half = step / 2;
    let stepped = ((clamped + half) / step) * step;
    stepped.clamp(MIN_TEMPERATURE, MAX_TEMPERATURE)
}

fn cursor_position() -> Option<(i32, i32)> {
    let device_state = DeviceQueryState::new();
    let mouse = device_state.get_mouse();
    Some(mouse.coords)
}

fn monitor_contains_point(monitor: &winit::monitor::MonitorHandle, x: i32, y: i32) -> bool {
    let position = monitor.position();
    let size = monitor.size();
    let max_x = position.x + size.width as i32;
    let max_y = position.y + size.height as i32;
    x >= position.x && x < max_x && y >= position.y && y < max_y
}

fn monitor_contains_point_logical(monitor: &winit::monitor::MonitorHandle, x: i32, y: i32) -> bool {
    let scale = monitor.scale_factor();
    let position = winit::dpi::PhysicalPosition::new(monitor.position().x, monitor.position().y);
    let size = winit::dpi::PhysicalSize::new(monitor.size().width, monitor.size().height);
    let logical_position = position.to_logical::<f64>(scale);
    let logical_size = size.to_logical::<f64>(scale);
    let max_x = logical_position.x + logical_size.width;
    let max_y = logical_position.y + logical_size.height;
    let x = x as f64;
    let y = y as f64;
    x >= logical_position.x && x < max_x && y >= logical_position.y && y < max_y
}

fn active_monitor(window: &winit::window::Window) -> Option<winit::monitor::MonitorHandle> {
    if let Some((x, y)) = cursor_position() {
        for monitor in window.available_monitors() {
            if monitor_contains_point(&monitor, x, y) {
                return Some(monitor);
            }
        }
        for monitor in window.available_monitors() {
            if monitor_contains_point_logical(&monitor, x, y) {
                return Some(monitor);
            }
        }
    }

    window
        .current_monitor()
        .or_else(|| window.primary_monitor())
        .or_else(|| window.available_monitors().next())
}

fn center_window_on_active_monitor(window: &winit::window::Window) -> bool {
    let monitor = active_monitor(window);
    let Some(monitor) = monitor else {
        return false;
    };

    let monitor_size = monitor.size();
    let mut window_size = window.outer_size();
    if window_size.width == 0 || window_size.height == 0 {
        return false;
    }
    if window_size.width > monitor_size.width || window_size.height > monitor_size.height {
        let inner_size = window.inner_size();
        if inner_size.width > 0
            && inner_size.height > 0
            && inner_size.width <= monitor_size.width
            && inner_size.height <= monitor_size.height
        {
            window_size = inner_size;
        }
    }

    let monitor_position = monitor.position();
    let x = monitor_position.x + ((monitor_size.width as i32 - window_size.width as i32) / 2);
    let y = monitor_position.y + ((monitor_size.height as i32 - window_size.height as i32) / 2);
    let max_x = monitor_position.x + (monitor_size.width as i32 - window_size.width as i32);
    let max_y = monitor_position.y + (monitor_size.height as i32 - window_size.height as i32);
    let clamped_x = if max_x < monitor_position.x {
        monitor_position.x
    } else {
        x.clamp(monitor_position.x, max_x)
    };
    let clamped_y = if max_y < monitor_position.y {
        monitor_position.y
    } else {
        y.clamp(monitor_position.y, max_y)
    };
    window.set_outer_position(winit::dpi::PhysicalPosition::new(clamped_x, clamped_y));
    true
}

fn schedule_center_window(app_weak: slint::Weak<AppWindow>, attempts_left: u8) {
    slint::Timer::single_shot(CENTER_RETRY_DELAY, move || {
        let Some(app) = app_weak.upgrade() else {
            return;
        };
        let did_center = app
            .window()
            .with_winit_window(center_window_on_active_monitor)
            .unwrap_or(false);
        if !did_center && attempts_left > 0 {
            schedule_center_window(app_weak.clone(), attempts_left - 1);
        }
    });
}

#[cfg(feature = "tray")]
enum TrayCommand {
    Show,
    Quit,
}

#[cfg(feature = "tray")]
fn setup_tray() -> Option<(tray_item::TrayItem, std_mpsc::Receiver<TrayCommand>)> {
    use tray_item::TrayItem;

    let tray_result = TrayItem::new("Litra Glow", tray_item::IconSource::Resource("tray-icon"));
    let mut tray = match tray_result {
        Ok(t) => t,
        Err(e) => {
            warn!(
                "Failed to create tray icon: {:?}. Tray functionality disabled.",
                e
            );
            return None;
        }
    };

    let (tx, rx) = std_mpsc::channel::<TrayCommand>();
    let tx_quit = tx.clone();

    if tray
        .add_menu_item("Show", move || {
            let _ = tx.send(TrayCommand::Show);
        })
        .is_err()
    {
        return None;
    }

    if tray
        .add_menu_item("Quit", move || {
            let _ = tx_quit.send(TrayCommand::Quit);
        })
        .is_err()
    {
        return None;
    }

    info!("Tray icon created successfully");
    Some((tray, rx))
}

#[cfg(feature = "tray")]
fn handle_tray_command(cmd: TrayCommand, app_weak: &slint::Weak<AppWindow>) {
    if let Some(app) = app_weak.upgrade() {
        match cmd {
            TrayCommand::Show => {
                app.window().with_winit_window(|w| {
                    center_window_on_active_monitor(w);
                    w.set_visible(true);
                    w.focus_window();
                });
                schedule_center_window(app.as_weak(), CENTER_RETRY_LIMIT);
            }
            TrayCommand::Quit => {
                slint::quit_event_loop().ok();
            }
        }
    }
}

#[cfg(windows)]
fn init_cli_console() {
    if std::env::args_os().nth(1).is_none() {
        return;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn AttachConsole(dw_process_id: u32) -> i32;
        fn GetStdHandle(n_std_handle: u32) -> isize;
        fn SetStdHandle(n_std_handle: u32, handle: isize) -> i32;
    }

    const STD_OUTPUT_HANDLE: u32 = -11i32 as u32;
    const STD_ERROR_HANDLE: u32 = -12i32 as u32;
    const INVALID_HANDLE_VALUE: isize = -1;
    const ATTACH_PARENT_PROCESS: u32 = 0xFFFFFFFF;

    unsafe {
        let stdout_handle = GetStdHandle(STD_OUTPUT_HANDLE);
        let stderr_handle = GetStdHandle(STD_ERROR_HANDLE);
        let stdout_invalid = stdout_handle == 0 || stdout_handle == INVALID_HANDLE_VALUE;
        let stderr_invalid = stderr_handle == 0 || stderr_handle == INVALID_HANDLE_VALUE;

        if stdout_invalid || stderr_invalid {
            let _ = AttachConsole(ATTACH_PARENT_PROCESS);
        }

        if stdout_invalid && let Ok(file) = std::fs::OpenOptions::new().write(true).open("CONOUT$")
        {
            let handle = file.as_raw_handle() as isize;
            let _ = SetStdHandle(STD_OUTPUT_HANDLE, handle);
            std::mem::forget(file);
        }

        if stderr_invalid && let Ok(file) = std::fs::OpenOptions::new().write(true).open("CONOUT$")
        {
            let handle = file.as_raw_handle() as isize;
            let _ = SetStdHandle(STD_ERROR_HANDLE, handle);
            std::mem::forget(file);
        }
    }
}

fn run_headless(cli: Cli) -> Result<(), String> {
    let device = LitraDevice::open().map_err(|e| format!("Failed to open device: {}", e))?;

    if cli.status {
        device.send(Command::GetPower).map_err(|e| e.to_string())?;
        thread::sleep(Duration::from_millis(100));
        device
            .send(Command::GetBrightness)
            .map_err(|e| e.to_string())?;
        thread::sleep(Duration::from_millis(100));
        device
            .send(Command::GetTemperature)
            .map_err(|e| e.to_string())?;

        let mut power = None;
        let mut brightness = None;
        let mut temperature = None;

        for _ in 0..10 {
            thread::sleep(Duration::from_millis(50));
            while let Ok(Some(response)) = device.try_read() {
                match response {
                    Response::Power(on, _) => power = Some(on),
                    Response::Brightness(level, _) => brightness = Some(level),
                    Response::Temperature(temp, _) => temperature = Some(temp),
                }
            }
            if power.is_some() && brightness.is_some() && temperature.is_some() {
                break;
            }
        }

        let brightness_pct =
            brightness.map(|b| ((b - MIN_BRIGHTNESS) * 100) / (MAX_BRIGHTNESS - MIN_BRIGHTNESS));

        fn fmt_opt<T: std::fmt::Display>(opt: Option<T>) -> String {
            opt.map_or("null".to_string(), |v| v.to_string())
        }
        println!(
            "{{\"power\":{},\"brightness\":{},\"temperature\":{}}}",
            fmt_opt(power),
            fmt_opt(brightness_pct),
            fmt_opt(temperature)
        );

        return Ok(());
    }

    if cli.toggle {
        device.send(Command::GetPower).map_err(|e| e.to_string())?;
        thread::sleep(Duration::from_millis(100));
        if let Ok(Some(Response::Power(on, _))) = device.try_read() {
            device
                .send(Command::SetPower(!on))
                .map_err(|e| e.to_string())?;
        }
    } else if cli.on {
        device
            .send(Command::SetPower(true))
            .map_err(|e| e.to_string())?;
    } else if cli.off {
        device
            .send(Command::SetPower(false))
            .map_err(|e| e.to_string())?;
    }

    if let Some(percent) = cli.brightness {
        let level = (percent as u16).clamp(0, 100);
        let brightness = MIN_BRIGHTNESS + (level * (MAX_BRIGHTNESS - MIN_BRIGHTNESS) / 100);
        device
            .send(Command::SetBrightness(brightness))
            .map_err(|e| e.to_string())?;
    }

    if let Some(temp) = cli.temperature {
        let temp = temp.clamp(MIN_TEMPERATURE, MAX_TEMPERATURE);
        device
            .send(Command::SetTemperature(temp))
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

fn main() -> Result<(), slint::PlatformError> {
    #[cfg(windows)]
    init_cli_console();

    let cli = Cli::parse();

    if cli.has_commands() {
        if let Err(e) = run_headless(cli) {
            eprintln!("Error: {}", e);
            std::process::exit(1);
        }
        return Ok(());
    }

    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    info!("Starting Litra Glow app");

    let app = AppWindow::new()?;
    info!("App window created");
    let app_weak_center = app.as_weak();
    slint::Timer::single_shot(Duration::from_millis(0), move || {
        schedule_center_window(app_weak_center, CENTER_RETRY_LIMIT);
    });

    let initialized = Rc::new(Cell::new(false));

    app.set_brightness(MIN_BRIGHTNESS as f32);
    app.set_temperature(MIN_TEMPERATURE as f32);
    app.set_power(false);
    app.set_error("Connecting...".into());

    #[cfg(feature = "tray")]
    let tray_setup = setup_tray();
    #[cfg(feature = "tray")]
    let tray_enabled = tray_setup.is_some();
    #[cfg(not(feature = "tray"))]
    let tray_enabled = false;

    let (cmd_tx, cmd_rx) = mpsc::channel();
    let (evt_tx, evt_rx) = mpsc::channel();

    let device_state = DeviceState {
        power: false,
        brightness: MIN_BRIGHTNESS,
        temperature: MIN_TEMPERATURE,
        pending_brightness: None,
        pending_temperature: None,
    };
    thread::spawn(move || device_loop(cmd_rx, evt_tx, device_state));

    let initialized_brightness = Rc::clone(&initialized);
    let cmd_tx_brightness = cmd_tx.clone();
    app.on_brightness_changed(move |value| {
        if !initialized_brightness.get() {
            return;
        }
        let level = clamp_brightness(value);
        info!("Brightness changed: {} -> {}", value, level);
        let _ = cmd_tx_brightness.send(DeviceCommand::SetBrightness(level));
    });

    let initialized_temperature = Rc::clone(&initialized);
    let cmd_tx_temperature = cmd_tx.clone();
    app.on_temperature_changed(move |value| {
        if !initialized_temperature.get() {
            return;
        }
        let level = clamp_temperature(value);
        info!("Temperature changed: {} -> {}", value, level);
        let _ = cmd_tx_temperature.send(DeviceCommand::SetTemperature(level));
    });

    let initialized_power = Rc::clone(&initialized);
    let cmd_tx_power = cmd_tx.clone();
    app.on_power_toggled(move |on| {
        if !initialized_power.get() {
            return;
        }
        info!("Power toggled: {}", on);
        let _ = cmd_tx_power.send(DeviceCommand::SetPower(on));
    });

    let cmd_tx_retry = cmd_tx.clone();
    app.on_retry_connect(move || {
        let _ = cmd_tx_retry.send(DeviceCommand::Retry);
    });

    let app_weak_minimize = app.as_weak();
    app.on_minimize(move || {
        if let Some(app) = app_weak_minimize.upgrade() {
            if tray_enabled {
                app.window().with_winit_window(|w| {
                    w.set_visible(false);
                });
            } else {
                app.window().set_minimized(true);
            }
        }
    });

    app.on_close(move || {
        let _ = slint::quit_event_loop();
    });

    app.on_donate(move || {
        let _ = open::that("https://github.com/sponsors/3axap4eHko");
    });

    let app_weak_drag = app.as_weak();
    app.on_start_drag(move || {
        let Some(app) = app_weak_drag.upgrade() else {
            return;
        };
        app.window().with_winit_window(|window| {
            let _ = window.drag_window();
        });
    });

    let app_weak_events = app.as_weak();
    let initialized_events = Rc::clone(&initialized);
    let init_count = Rc::new(Cell::new(0u8));
    let timer = slint::Timer::default();
    timer.start(
        slint::TimerMode::Repeated,
        Duration::from_millis(50),
        move || {
            let Some(app) = app_weak_events.upgrade() else {
                return;
            };

            #[cfg(feature = "tray")]
            if let Some((ref _tray, ref tray_rx)) = tray_setup {
                while let Ok(cmd) = tray_rx.try_recv() {
                    handle_tray_command(cmd, &app_weak_events);
                }
            }

            while let Ok(event) = evt_rx.try_recv() {
                match event {
                    DeviceEvent::Connected => {
                        app.set_error("".into());
                    }
                    DeviceEvent::Power(on) => {
                        info!("UI received power event: {}", on);
                        app.set_power(on);
                        if !initialized_events.get() {
                            init_count.set(init_count.get() + 1);
                        }
                    }
                    DeviceEvent::Brightness(level) => {
                        app.set_brightness(level as f32);
                        if !initialized_events.get() {
                            init_count.set(init_count.get() + 1);
                        }
                    }
                    DeviceEvent::Temperature(level) => {
                        app.set_temperature(level as f32);
                        if !initialized_events.get() {
                            init_count.set(init_count.get() + 1);
                        }
                    }
                    DeviceEvent::Error(message) => {
                        app.set_error(message.into());
                    }
                }
                if !initialized_events.get() && init_count.get() >= 2 {
                    info!("Initialization complete");
                    initialized_events.set(true);
                }
            }
        },
    );

    app.run()
}

fn device_loop(
    cmd_rx: mpsc::Receiver<DeviceCommand>,
    evt_tx: mpsc::Sender<DeviceEvent>,
    mut state: DeviceState,
) {
    info!("Device loop started");
    let mut device: Option<LitraDevice> = None;
    let mut last_error: Option<String> = None;

    loop {
        if device.is_none() {
            debug!("Trying to open device...");
            match LitraDevice::open() {
                Ok(dev) => {
                    info!("Device connected, querying state...");
                    if let Err(e) = dev.send(Command::GetPower) {
                        error!("Failed to send GetPower: {}", e);
                    }
                    thread::sleep(Duration::from_millis(100));
                    if let Err(e) = dev.send(Command::GetBrightness) {
                        error!("Failed to send GetBrightness: {}", e);
                    }
                    thread::sleep(Duration::from_millis(100));
                    if let Err(e) = dev.send(Command::GetTemperature) {
                        error!("Failed to send GetTemperature: {}", e);
                    }
                    device = Some(dev);
                    last_error = None;
                    let _ = evt_tx.send(DeviceEvent::Connected);
                }
                Err(err) => {
                    let message = err.to_string();
                    if last_error.as_deref() != Some(&message) {
                        warn!("Device error: {}", message);
                        let _ = evt_tx.send(DeviceEvent::Error(message.clone()));
                        last_error = Some(message);
                    }
                    match cmd_rx.recv_timeout(Duration::from_secs(2)) {
                        Ok(cmd) => {
                            debug!("Received command while disconnected: {:?}", cmd);
                            let _ = handle_command(cmd, &mut state, None);
                        }
                        Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {}
                    }
                    continue;
                }
            }
        }

        let mut disconnected = false;
        if let Some(dev) = device.as_ref() {
            while let Ok(cmd) = cmd_rx.try_recv() {
                info!("Received command: {:?}", cmd);
                if handle_command(cmd, &mut state, Some(dev)).is_err() {
                    error!("Command failed, device disconnected");
                    disconnected = true;
                    break;
                }
            }

            if !disconnected {
                match dev.try_read() {
                    Ok(Some(response)) => {
                        debug!("Received response: {:?}", response);
                        match response {
                            Response::Power(on, _) => {
                                state.power = on;
                                info!("Sending power event to UI: {}", on);
                                let _ = evt_tx.send(DeviceEvent::Power(on));
                            }
                            Response::Brightness(level, is_hw) => {
                                let accept = is_hw
                                    || match state.pending_brightness {
                                        Some(t) if t.elapsed() < PENDING_TIMEOUT => false,
                                        Some(_) => {
                                            state.pending_brightness = None;
                                            true
                                        }
                                        None => true,
                                    };
                                if accept {
                                    state.brightness = level;
                                    let _ = evt_tx.send(DeviceEvent::Brightness(level));
                                }
                            }
                            Response::Temperature(level, is_hw) => {
                                let accept = is_hw
                                    || match state.pending_temperature {
                                        Some(t) if t.elapsed() < PENDING_TIMEOUT => false,
                                        Some(_) => {
                                            state.pending_temperature = None;
                                            true
                                        }
                                        None => true,
                                    };
                                if accept {
                                    state.temperature = level;
                                    let _ = evt_tx.send(DeviceEvent::Temperature(level));
                                }
                            }
                        }
                    }
                    Ok(None) => {}
                    Err(e) => {
                        error!("Read error: {:?}", e);
                        disconnected = true;
                    }
                }
            }
        }

        if disconnected {
            warn!("Device disconnected");
            device = None;
            let _ = evt_tx.send(DeviceEvent::Error("Device disconnected".to_string()));
        }

        thread::sleep(Duration::from_millis(30));
    }
}

fn handle_command(
    cmd: DeviceCommand,
    state: &mut DeviceState,
    device: Option<&LitraDevice>,
) -> Result<(), usb::Error> {
    match cmd {
        DeviceCommand::Retry => {}
        DeviceCommand::SetPower(on) => {
            state.power = on;
            if let Some(dev) = device {
                dev.send(Command::SetPower(on))?;
            }
        }
        DeviceCommand::SetBrightness(level) => {
            state.brightness = level;
            state.pending_brightness = Some(Instant::now());
            if let Some(dev) = device {
                dev.send(Command::SetBrightness(level))?;
            }
        }
        DeviceCommand::SetTemperature(level) => {
            state.temperature = level;
            state.pending_temperature = Some(Instant::now());
            if let Some(dev) = device {
                dev.send(Command::SetTemperature(level))?;
            }
        }
    }

    Ok(())
}
