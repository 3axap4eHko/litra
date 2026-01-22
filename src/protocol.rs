pub const VENDOR_ID: u16 = 0x046d;
pub const PRODUCT_ID: u16 = 0xc900;

pub const MIN_BRIGHTNESS: u16 = 0x14;
pub const MAX_BRIGHTNESS: u16 = 0xfa;
pub const MIN_TEMPERATURE: u16 = 2700;
pub const MAX_TEMPERATURE: u16 = 6500;
pub const TEMPERATURE_STEP: u16 = 100;

const SET_POWER: u32 = 0x11FF041C;
const SET_BRIGHTNESS: u32 = 0x11FF044C;
const SET_TEMPERATURE: u32 = 0x11FF049C;

const GET_POWER: u32 = 0x11FF0401;
const GET_BRIGHTNESS: u32 = 0x11FF0431;
const GET_TEMPERATURE: u32 = 0x11FF0481;

#[derive(Debug)]
pub enum Command {
    SetPower(bool),
    SetBrightness(u16),
    SetTemperature(u16),
    GetPower,
    GetBrightness,
    GetTemperature,
}

impl Command {
    pub fn to_bytes(&self) -> [u8; 20] {
        let mut buf = [0u8; 20];
        match self {
            Command::SetPower(on) => {
                buf[0..4].copy_from_slice(&SET_POWER.to_be_bytes());
                buf[4] = *on as u8;
            }
            Command::SetBrightness(level) => {
                buf[0..4].copy_from_slice(&SET_BRIGHTNESS.to_be_bytes());
                buf[4..6].copy_from_slice(&level.to_be_bytes());
            }
            Command::SetTemperature(kelvin) => {
                buf[0..4].copy_from_slice(&SET_TEMPERATURE.to_be_bytes());
                buf[4..6].copy_from_slice(&kelvin.to_be_bytes());
            }
            Command::GetPower => {
                buf[0..4].copy_from_slice(&GET_POWER.to_be_bytes());
            }
            Command::GetBrightness => {
                buf[0..4].copy_from_slice(&GET_BRIGHTNESS.to_be_bytes());
            }
            Command::GetTemperature => {
                buf[0..4].copy_from_slice(&GET_TEMPERATURE.to_be_bytes());
            }
        }
        buf
    }
}

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum Response {
    Power(bool, bool),
    Brightness(u16, bool),
    Temperature(u16, bool),
}

impl Response {
    pub fn from_bytes(data: &[u8]) -> Option<Self> {
        if data.len() < 6 {
            return None;
        }
        match data[3] {
            0x00 => Some(Response::Power(data[4] != 0, true)),
            0x01 => Some(Response::Power(data[4] != 0, false)),
            0x10 => Some(Response::Brightness(data[5] as u16, true)),
            0x31 => Some(Response::Brightness(data[5] as u16, false)),
            0x20 => {
                let temp = u16::from_be_bytes([data[4], data[5]]);
                Some(Response::Temperature(temp, true))
            }
            0x81 => {
                let temp = u16::from_be_bytes([data[4], data[5]]);
                Some(Response::Temperature(temp, false))
            }
            _ => None,
        }
    }
}
