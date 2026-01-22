use hidapi::{HidApi, HidDevice};
use log::info;

use crate::protocol::{Command, PRODUCT_ID, Response, VENDOR_ID};

#[derive(Debug)]
pub enum Error {
    DeviceNotFound,
    Hid(hidapi::HidError),
}

impl From<hidapi::HidError> for Error {
    fn from(e: hidapi::HidError) -> Self {
        Error::Hid(e)
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::DeviceNotFound => write!(f, "Litra device not found"),
            Error::Hid(e) => write!(f, "HID error: {e}"),
        }
    }
}

impl std::error::Error for Error {}

pub struct LitraDevice {
    device: HidDevice,
}

impl LitraDevice {
    pub fn open() -> Result<Self, Error> {
        info!("Initializing HID API...");
        let api = HidApi::new()?;

        info!(
            "Looking for device VID={:04x} PID={:04x}",
            VENDOR_ID, PRODUCT_ID
        );
        let device = api
            .open(VENDOR_ID, PRODUCT_ID)
            .map_err(|_| Error::DeviceNotFound)?;

        info!("Device opened successfully");
        device.set_blocking_mode(false)?;

        Ok(Self { device })
    }

    pub fn send(&self, cmd: Command) -> Result<(), Error> {
        let data = cmd.to_bytes();
        info!("Sending {:?}: {:02x?}", cmd, &data[..8]);
        let written = self.device.write(&data)?;
        info!("Wrote {} bytes", written);
        Ok(())
    }

    pub fn try_read(&self) -> Result<Option<Response>, Error> {
        let mut buf = [0u8; 64];
        match self.device.read_timeout(&mut buf, 50) {
            Ok(0) => Ok(None),
            Ok(len) => {
                info!("Read {} bytes: {:02x?}", len, &buf[..len.min(16)]);
                let response = Response::from_bytes(&buf[..len]);
                info!("Parsed response: {:?}", response);
                Ok(response)
            }
            Err(e) => Err(e.into()),
        }
    }
}
