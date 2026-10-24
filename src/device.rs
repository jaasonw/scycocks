use crate::protocol::{self, REPORT_ID, REPORT_LEN};
use hidapi::{HidApi, HidDevice};
use std::ffi::CString;
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct DeviceInfo {
    pub path: CString,
    pub label: String,
}

pub fn enumerate() -> Result<Vec<DeviceInfo>, String> {
    let api = HidApi::new().map_err(|e| e.to_string())?;
    Ok(api
        .device_list()
        .filter(|d| {
            protocol::supported(d.vendor_id(), d.product_id())
                && d.usage_page() == 0xff02
                && d.usage() == 2
        })
        .map(|d| DeviceInfo {
            path: d.path().to_owned(),
            label: format!(
                "{}  [{:04X}:{:04X}, usage {:04X}:{:04X}]",
                d.product_string().unwrap_or("Scyrox"),
                d.vendor_id(),
                d.product_id(),
                d.usage_page(),
                d.usage()
            ),
        })
        .collect())
}

pub struct Mouse {
    device: HidDevice,
    pub identity: [u8; 3],
}

impl Mouse {
    pub fn open(path: &CString) -> Result<Self, String> {
        let api = HidApi::new().map_err(|e| e.to_string())?;
        let info = api
            .device_list()
            .find(|d| d.path() == path.as_c_str())
            .ok_or("The selected device disconnected. Refresh the device list.")?;
        if !protocol::supported(info.vendor_id(), info.product_id())
            || info.usage_page() != 0xff02
            || info.usage() != 2
        {
            return Err("The selected interface is not a supported Scyrox interface.".into());
        }
        let device = info.open_device(&api).map_err(|e| e.to_string())?;
        let mut mouse = Self {
            device,
            identity: [0; 3],
        };
        let reply = mouse.exchange(protocol::packet(
            1,
            0,
            8,
            &[0x17, 0x39, 0x53, 0x71, 0, 0, 0, 0],
        )?)?;
        mouse.identity.copy_from_slice(&reply[10..13]);
        if mouse.identity[0] != 62 || !matches!(mouse.identity[1], 1 | 10 | 11) {
            return Err(format!(
                "Unsupported model identity {:?}. Only the Scyrox V8 is enabled.",
                mouse.identity
            ));
        }
        if !matches!(mouse.identity[2], 0..=5) {
            return Err("Unknown connection type. No settings were changed.".into());
        }
        Ok(mouse)
    }

    fn exchange(&self, request: [u8; REPORT_LEN]) -> Result<[u8; REPORT_LEN], String> {
        self.exchange_wait(request, Duration::from_millis(800))
    }

    fn exchange_wait(
        &self,
        request: [u8; REPORT_LEN],
        timeout: Duration,
    ) -> Result<[u8; REPORT_LEN], String> {
        // Reads can generate unsolicited status reports. Match the complete request header.
        let sent = self.device.write(&request).map_err(|e| e.to_string())?;
        if sent != request.len() {
            return Err("The HID report was only partly sent.".into());
        }
        let deadline = Instant::now() + timeout;
        let mut buffer = [0; 64];
        while Instant::now() < deadline {
            let remaining = deadline
                .saturating_duration_since(Instant::now())
                .as_millis()
                .min(800) as i32;
            let size = self
                .device
                .read_timeout(&mut buffer, remaining.max(1))
                .map_err(|e| e.to_string())?;
            if size == 0 {
                continue;
            }
            if size < REPORT_LEN || buffer[0] != REPORT_ID || buffer[1] != request[1] {
                continue;
            }
            if buffer[2] != 0 {
                return Err(format!(
                    "The mouse rejected command 0x{:02X} (status {}).",
                    request[1], buffer[2]
                ));
            }
            let header = if matches!(request[1], 7 | 8) { 6 } else { 5 };
            if buffer[3..header] != request[3..header] {
                continue;
            }
            let mut reply = [0; REPORT_LEN];
            reply.copy_from_slice(&buffer[..REPORT_LEN]);
            protocol::validate_reply(&reply)?;
            return Ok(reply);
        }
        Err(format!(
            "No reply to command 0x{:02X}. Close the Scyrox website or wake the mouse, then reconnect.",
            request[1]
        ))
    }

    pub fn query(&self, command: u8) -> Result<[u8; REPORT_LEN], String> {
        if !matches!(command, 3 | 4 | 6 | 14 | 18 | 29) {
            return Err("This query is not allowed.".into());
        }
        self.exchange(protocol::packet(command, 0, 0, &[])?)
    }

    pub fn check_online(&self) -> Result<(), String> {
        if self.query(3)?[6] != 1 {
            return Err("The receiver is connected, but the mouse is offline.".into());
        }
        Ok(())
    }

    pub fn read_config(&self) -> Result<[u8; 256], String> {
        self.check_online()?;
        let mut config = [0; 256];
        for address in (0..256).step_by(10) {
            let length = (256 - address).min(10);
            let reply = self.exchange(protocol::packet(8, address as u16, length as u8, &[])?)?;
            config[address..address + length].copy_from_slice(&reply[6..6 + length]);
        }
        Ok(config)
    }

    #[cfg(feature = "gui")]
    pub fn write_block(&self, address: u16, data: &[u8]) -> Result<(), String> {
        self.exchange(protocol::packet(
            7,
            address,
            data.len().try_into().map_err(|_| "Data is too long.")?,
            data,
        )?)?;
        let reply = self.exchange(protocol::packet(8, address, data.len() as u8, &[])?)?;
        if &reply[6..6 + data.len()] != data {
            return Err(format!(
                "Readback failed at configuration offset {address}. Refresh before further changes."
            ));
        }
        Ok(())
    }

    #[cfg(feature = "gui")]
    pub fn read_shortcut(&self, index: usize) -> Result<[u8; 32], String> {
        if index >= 6 {
            return Err("Invalid button index.".into());
        }
        let mut bytes = [0; 32];
        for (chunk, data) in bytes.chunks_mut(10).enumerate() {
            let reply = self.exchange(protocol::packet(
                8,
                (256 + index * 32 + chunk * 10) as u16,
                data.len() as u8,
                &[],
            )?)?;
            data.copy_from_slice(&reply[6..6 + data.len()]);
        }
        Ok(bytes)
    }

    #[cfg(feature = "gui")]
    pub fn set_profile(&self, profile: u8) -> Result<(), String> {
        if profile > 3 {
            return Err("Select profile 1–4.".into());
        }
        self.exchange(protocol::packet(15, 0, 1, &[profile])?)?;
        if self.query(14)?[6] != profile {
            return Err("Profile switch was not verified.".into());
        }
        Ok(())
    }

    #[cfg(feature = "gui")]
    pub fn factory_reset(&self) -> Result<(), String> {
        self.exchange_wait(protocol::packet(9, 0, 0, &[])?, Duration::from_secs(6))?;
        Ok(())
    }

    #[cfg(feature = "gui")]
    pub fn start_pair(&self) -> Result<(), String> {
        if matches!(self.identity[2], 2 | 3) {
            return Err("Pairing requires the wireless receiver, not a wired mouse.".into());
        }
        self.exchange(protocol::pairing_packet())?;
        Ok(())
    }

    pub fn max_rate(&self) -> u16 {
        match self.identity[2] {
            0 | 2 => 1000,
            1 => 4000,
            4 => 2000,
            _ => 8000,
        }
    }
}
