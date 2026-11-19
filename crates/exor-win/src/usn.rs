//! Portable parser for `FSCTL_READ_USN_JOURNAL` output (USN_RECORD_V2).

use exor_engine::events::FsReason;

/// A parsed USN record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsnRecord {
    /// File reference number.
    pub frn: u64,
    /// Parent directory reference number.
    pub parent_frn: u64,
    /// Update sequence number.
    pub usn: i64,
    /// FILETIME of the change.
    pub filetime: i64,
    /// Raw reason mask.
    pub reason: u32,
    /// File name (no directory).
    pub name: String,
}

impl UsnRecord {
    /// Collapsed reason, `None` for records that carry no change (e.g. CLOSE only).
    pub fn fs_reason(&self) -> Option<FsReason> {
        FsReason::from_usn_reason(self.reason)
    }
}

const V2_MIN: usize = 60;

fn rd<const N: usize>(b: &[u8], o: usize) -> Option<[u8; N]> {
    b.get(o..o + N)?.try_into().ok()
}

/// Parse a read buffer: 8-byte next-USN followed by V2 records. Malformed or non-V2
/// records end parsing; they are never read out of bounds.
pub fn parse_usn_buffer(buf: &[u8]) -> (i64, Vec<UsnRecord>) {
    let next = rd::<8>(buf, 0).map_or(0, i64::from_le_bytes);
    let mut out = Vec::new();
    let mut off = 8usize;
    while off + V2_MIN <= buf.len() {
        let Some(len) = rd::<4>(buf, off)
            .map(u32::from_le_bytes)
            .map(|l| l as usize)
        else {
            break;
        };
        if len < V2_MIN || off + len > buf.len() {
            break;
        }
        let rec = &buf[off..off + len];
        let major = rd::<2>(rec, 4).map_or(0, u16::from_le_bytes);
        if major == 2 {
            let name_len = rd::<2>(rec, 56).map_or(0, u16::from_le_bytes) as usize;
            let name_off = rd::<2>(rec, 58).map_or(0, u16::from_le_bytes) as usize;
            if let Some(nb) = rec.get(name_off..name_off + name_len)
                && nb.len().is_multiple_of(2)
            {
                let units: Vec<u16> = nb
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .collect();
                out.push(UsnRecord {
                    frn: rd::<8>(rec, 8).map_or(0, u64::from_le_bytes),
                    parent_frn: rd::<8>(rec, 16).map_or(0, u64::from_le_bytes),
                    usn: rd::<8>(rec, 24).map_or(0, i64::from_le_bytes),
                    filetime: rd::<8>(rec, 32).map_or(0, i64::from_le_bytes),
                    reason: rd::<4>(rec, 40).map_or(0, u32::from_le_bytes),
                    name: String::from_utf16_lossy(&units),
                });
            }
        }
        off += len;
    }
    (next, out)
}

/// Check the kernel buffer for completeness; unlike the forgiving parser this reports gaps.
pub fn checked_usn_buffer(buf: &[u8]) -> Option<(i64, Vec<UsnRecord>)> {
    let mut offset = 8;
    if buf.len() < offset {
        return None;
    }
    while offset < buf.len() {
        let len = u32::from_le_bytes(rd::<4>(buf, offset)?) as usize;
        let rec = buf.get(offset..offset.checked_add(len)?)?;
        if len < V2_MIN || !len.is_multiple_of(8) || u16::from_le_bytes(rd::<2>(rec, 4)?) != 2 {
            return None;
        }
        let size = u16::from_le_bytes(rd::<2>(rec, 56)?) as usize;
        let start = u16::from_le_bytes(rd::<2>(rec, 58)?) as usize;
        if start < V2_MIN || !size.is_multiple_of(2) || start + size > len {
            return None;
        }
        offset += len;
    }
    Some(parse_usn_buffer(buf))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used)]
mod tests {
    use super::*;

    fn record(frn: u64, parent: u64, usn: i64, reason: u32, name: &str) -> Vec<u8> {
        let nb: Vec<u8> = name.encode_utf16().flat_map(u16::to_le_bytes).collect();
        let len = (V2_MIN + nb.len()).next_multiple_of(8);
        let mut r = vec![0u8; len];
        r[0..4].copy_from_slice(&(len as u32).to_le_bytes());
        r[4..6].copy_from_slice(&2u16.to_le_bytes());
        r[8..16].copy_from_slice(&frn.to_le_bytes());
        r[16..24].copy_from_slice(&parent.to_le_bytes());
        r[24..32].copy_from_slice(&usn.to_le_bytes());
        r[40..44].copy_from_slice(&reason.to_le_bytes());
        r[56..58].copy_from_slice(&(nb.len() as u16).to_le_bytes());
        r[58..60].copy_from_slice(&(V2_MIN as u16).to_le_bytes());
        r[V2_MIN..V2_MIN + nb.len()].copy_from_slice(&nb);
        r
    }

    #[test]
    fn parses_two_records_and_next_usn() {
        let mut buf = 777i64.to_le_bytes().to_vec();
        buf.extend(record(10, 5, 100, 0x100, "a.dll"));
        buf.extend(record(11, 5, 200, 0x200, "b.dll"));
        let (next, recs) = parse_usn_buffer(&buf);
        assert_eq!(next, 777);
        assert_eq!(recs.len(), 2);
        assert_eq!(recs[0].name, "a.dll");
        assert_eq!(recs[0].fs_reason(), Some(FsReason::Create));
        assert_eq!(recs[1].fs_reason(), Some(FsReason::Delete));
        assert_eq!((recs[1].frn, recs[1].parent_frn, recs[1].usn), (11, 5, 200));
    }

    #[test]
    fn garbage_never_panics_or_overreads() {
        let mut buf = 1i64.to_le_bytes().to_vec();
        let mut r = record(1, 1, 1, 0x100, "x");
        r[0..4].copy_from_slice(&0xFFFF_FFF0u32.to_le_bytes());
        buf.extend(r);
        assert!(parse_usn_buffer(&buf).1.is_empty());
        assert!(checked_usn_buffer(&buf).is_none());
        assert!(parse_usn_buffer(&[]).1.is_empty());
        let mut r2 = record(1, 1, 1, 0x100, "x");
        r2[58..60].copy_from_slice(&0xFFF0u16.to_le_bytes()); // name offset outside record
        let mut b2 = 1i64.to_le_bytes().to_vec();
        b2.extend(r2);
        assert!(parse_usn_buffer(&b2).1.is_empty());
    }
}
