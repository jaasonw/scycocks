//! Chip families: the Rust half of a driver. A family opens the HID interface, frames and
//! checks reports, and supplies codecs for encodings a device file cannot express as data.

use crate::driver::codec::Codec;
use std::time::Duration;

mod endgame;
mod mchose;
mod scyrox;

pub trait Link: Send {
    /// Sends one frame built from a device-file template and returns the reply payload.
    /// With `reply` false the family may return an empty payload once the device acknowledges.
    fn exchange(&self, frame: &[u8], reply: bool, timeout: Duration) -> Result<Vec<u8>, String>;
}

type Open = fn(&hidapi::HidApi, &hidapi::DeviceInfo) -> Result<Box<dyn Link>, String>;

pub struct Family {
    pub name: &'static str,
    pub open: Open,
    pub codec: fn(&str) -> Option<Codec>,
}

pub static FAMILIES: &[Family] = &[scyrox::FAMILY, mchose::FAMILY, endgame::FAMILY];

pub fn find(name: &str) -> Option<&'static Family> {
    FAMILIES.iter().find(|family| family.name == name)
}
