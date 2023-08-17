pub fn tlog_main() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

use anyhow::{anyhow, Result};

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum PayloadType {
    Debug = 0,
    Warning = 1,
    Error = 2,
    Unknown,
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct TLog {
    payload_type: PayloadType,
    payload: String,
}

impl TLog {
    pub fn new(payload: String, payload_type: PayloadType) -> Self {
        Self {
            payload,
            payload_type,
        }
    }

    pub fn to_packet(&self) -> Result<Vec<u8>> {
        if self.payload.len() > u16::MAX.into() {
            return Err(anyhow!("Too large packet!"));
        }
        let p_type: u8 = match self.payload_type {
            PayloadType::Debug => 0x0,
            PayloadType::Warning => 0x1,
            PayloadType::Error => 0x2,
            PayloadType::Unknown => return Err(anyhow!("Invalid payload type!")),
        };

        let payload_len = (self.payload.len() as u16).to_be_bytes();
        Ok([
            vec![0x1A, payload_len[0], payload_len[1], 0x1, p_type],
            self.payload.as_bytes().to_vec(),
        ]
        .concat())
    }

    pub fn from_be_bytes(data_packet: Vec<u8>) -> Result<Self> {
        // Check initial conditions
        if data_packet.is_empty() {
            return Err(anyhow!("Input is empty"));
        }
        if data_packet[0] != 0x1A || data_packet[3] != 0x1 {
            return Err(anyhow!("Invalid or unsupported input format"));
        }

        let payload_len = u16::from_be_bytes([data_packet[1], data_packet[2]]) as usize;

        // Ensure packet size consistency
        if data_packet.len() != payload_len + 5 {
            return Err(anyhow!("Inconsistent data packet length"));
        }

        let payload_type = match data_packet[4] {
            0 => PayloadType::Debug,
            1 => PayloadType::Warning,
            2 => PayloadType::Error,
            _ => PayloadType::Unknown,
        };

        let payload = std::str::from_utf8(&data_packet[5..(payload_len + 5)])
            .map_err(|e| anyhow!("UTF8 conversion error: {}", e))?
            .to_owned();

        Ok(Self {
            payload_type,
            payload,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ascii() {
        let tlog = TLog::new(
            "I want to some very nice food!".to_owned(),
            PayloadType::Debug,
        );
        let packet = tlog.to_packet().unwrap();
        let out_log = TLog::from_be_bytes(packet).unwrap();

        assert_eq!(tlog, out_log);
    }

    #[test]
    fn test_utf8() {
        let tlog = TLog::new(
            "素早い茶色のキツネが怠惰な犬を飛び越える".to_owned(),
            PayloadType::Debug,
        );
        let packet = tlog.to_packet().unwrap();
        let out_log = TLog::from_be_bytes(packet).unwrap();

        assert_eq!(tlog, out_log);
    }

    #[test]
    fn test_utf8_icon() {
        let tlog = TLog::new("".to_owned(), PayloadType::Debug);
        let packet = tlog.to_packet().unwrap();
        let out_log = TLog::from_be_bytes(packet).unwrap();

        assert_eq!(tlog, out_log);
    }
}
