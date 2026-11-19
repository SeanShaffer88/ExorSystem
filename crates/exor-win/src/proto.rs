//! Wire format between `driver/exordrv.c` and user mode (little-endian, packed).
//!
//! ```text
//! offset size field
//!   0     4   total_len  (header + payload)
//!   4     2   kind       (1 = registry)
//!   6     2   op         (1 create key, 2 delete key, 3 set value, 4 delete value)
//!   8     8   filetime   (100 ns ticks since 1601-01-01 UTC)
//!  16     4   pid
//!  20     2   key_bytes  (UTF-16LE, no NUL)
//!  22     2   value_bytes(UTF-16LE, no NUL)
//!  24     2   proc_bytes (ASCII image name)
//!  26     2   reserved
//!  28     ..  key | value | process
//! ```

use crate::WinError;
use exor_engine::events::{RawEvent, RegOp};

/// Communication port name created by the driver.
pub const PORT_NAME: &str = "\\ExorSystemPort";
/// Fixed header size.
pub const HEADER_LEN: usize = 28;
/// Message kind: registry.
pub const KIND_REGISTRY: u16 = 1;

fn u16_at(b: &[u8], o: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(o..o + 2)?.try_into().ok()?))
}
fn u32_at(b: &[u8], o: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(o..o + 4)?.try_into().ok()?))
}
fn u64_at(b: &[u8], o: usize) -> Option<u64> {
    Some(u64::from_le_bytes(b.get(o..o + 8)?.try_into().ok()?))
}

fn utf16(b: &[u8]) -> Result<String, WinError> {
    if !b.len().is_multiple_of(2) {
        return Err(WinError::Proto("odd UTF-16 length"));
    }
    let units: Vec<u16> = b
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    Ok(String::from_utf16_lossy(&units))
}

/// Parse one registry message. Every length is bounds-checked against the buffer.
pub fn parse_registry_message(buf: &[u8]) -> Result<RawEvent, WinError> {
    let bad = WinError::Proto("truncated header");
    let total = u32_at(buf, 0).ok_or(WinError::Proto("truncated header"))? as usize;
    if total < HEADER_LEN || total > buf.len() {
        return Err(WinError::Proto("bad total length"));
    }
    if u16_at(buf, 4).ok_or(WinError::Proto("truncated header"))? != KIND_REGISTRY {
        return Err(WinError::Proto("unknown message kind"));
    }
    let op = match u16_at(buf, 6).ok_or(WinError::Proto("truncated header"))? {
        1 => RegOp::CreateKey,
        2 => RegOp::DeleteKey,
        3 => RegOp::SetValue,
        4 => RegOp::DeleteValue,
        _ => return Err(WinError::Proto("unknown registry op")),
    };
    let filetime = u64_at(buf, 8).ok_or(bad)?;
    let key_n = usize::from(u16_at(buf, 20).ok_or(WinError::Proto("truncated header"))?);
    let val_n = usize::from(u16_at(buf, 22).ok_or(WinError::Proto("truncated header"))?);
    let proc_n = usize::from(u16_at(buf, 24).ok_or(WinError::Proto("truncated header"))?);
    if HEADER_LEN + key_n + val_n + proc_n > total {
        return Err(WinError::Proto("payload exceeds message"));
    }
    let key = utf16(&buf[HEADER_LEN..HEADER_LEN + key_n])?;
    let value = utf16(&buf[HEADER_LEN + key_n..HEADER_LEN + key_n + val_n])?;
    let proc_b = &buf[HEADER_LEN + key_n + val_n..HEADER_LEN + key_n + val_n + proc_n];
    let process = (!proc_b.is_empty()).then(|| String::from_utf8_lossy(proc_b).into_owned());
    Ok(RawEvent::Registry {
        key,
        value_name: (!value.is_empty() || matches!(op, RegOp::SetValue | RegOp::DeleteValue))
            .then_some(value),
        op,
        process,
        ts_ms: crate::filetime_to_unix_ms(filetime),
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    pub(crate) fn build(op: u16, key: &str, value: &str, proc_name: &str) -> Vec<u8> {
        let k: Vec<u8> = key.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let v: Vec<u8> = value.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let total = HEADER_LEN + k.len() + v.len() + proc_name.len();
        let mut b = Vec::new();
        b.extend((total as u32).to_le_bytes());
        b.extend(KIND_REGISTRY.to_le_bytes());
        b.extend(op.to_le_bytes());
        b.extend(116_444_737_000_000_000u64.to_le_bytes());
        b.extend(4242u32.to_le_bytes());
        b.extend((k.len() as u16).to_le_bytes());
        b.extend((v.len() as u16).to_le_bytes());
        b.extend((proc_name.len() as u16).to_le_bytes());
        b.extend(0u16.to_le_bytes());
        b.extend(k);
        b.extend(v);
        b.extend(proc_name.as_bytes());
        b
    }

    #[test]
    fn parses_set_value() {
        let m = build(3, "\\REGISTRY\\MACHINE\\SOFTWARE\\Foo", "Ver", "setup.exe");
        let RawEvent::Registry {
            key,
            value_name,
            op,
            process,
            ts_ms,
        } = parse_registry_message(&m).unwrap()
        else {
            panic!("wrong variant")
        };
        assert_eq!(key, "\\REGISTRY\\MACHINE\\SOFTWARE\\Foo");
        assert_eq!(value_name.as_deref(), Some("Ver"));
        assert_eq!(op, RegOp::SetValue);
        assert_eq!(process.as_deref(), Some("setup.exe"));
        assert_eq!(ts_ms, 100_000);
    }

    #[test]
    fn create_key_has_no_value_name() {
        let m = build(1, "K", "", "");
        let RawEvent::Registry {
            value_name,
            process,
            ..
        } = parse_registry_message(&m).unwrap()
        else {
            panic!()
        };
        assert!(value_name.is_none() && process.is_none());
    }

    #[test]
    fn rejects_malformed() {
        assert!(parse_registry_message(&[]).is_err());
        let mut m = build(3, "K", "V", "p");
        m[0] = 0xFF; // total_len beyond buffer
        assert!(parse_registry_message(&m).is_err());
        let mut m = build(9, "K", "V", "p");
        assert!(parse_registry_message(&m).is_err());
        m = build(3, "K", "V", "p");
        m[20] = 0xFF; // key_bytes larger than payload
        assert!(parse_registry_message(&m).is_err());
        m = build(3, "K", "V", "p");
        m.truncate(m.len() - 1);
        assert!(parse_registry_message(&m).is_err());
    }
}
