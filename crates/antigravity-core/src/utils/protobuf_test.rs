#[cfg(test)]
mod tests {
    use super::super::protobuf::*;

    #[test]
    fn test_truncated_varint() {
        let data = vec![0x80, 0x80];
        let res = read_varint(&data, 0);
        assert!(res.is_err(), "Should return error for truncated varint");
        assert_eq!(res.unwrap_err(), "datanotcomplete");
    }

    #[test]
    fn test_truncated_64bit() {
        let data = vec![0x09, 0x01, 0x02]; // Tag 1, Wire 1 (64-bit), but only 2 bytes
        let res = skip_field(&data, 1, 1);
        assert!(res.is_err(), "Should return error for truncated 64-bit field");
    }

    #[test]
    fn test_truncated_length_delimited() {
        let data = vec![0x12, 0x05, 0x01, 0x02]; // Tag 2, Wire 2, Length 5, but only 2 bytes
        let res = skip_field(&data, 1, 2);
        assert!(res.is_err(), "Should return error for truncated length-delimited field");
    }

    #[test]
    fn test_length_overflow() {
        // Tag 2, Wire 2, Length = u64::MAX
        let mut data = vec![0x12];
        data.extend_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0x01]);
        let res = skip_field(&data, 1, 2);
        assert!(res.is_err(), "Should return error for length overflow");
    }

    #[test]
    fn test_invalid_wire_type() {
        let data = vec![0x0B]; // Tag 1, Wire 3 (deprecated/unhandled)
        let res = skip_field(&data, 1, 3);
        assert!(res.is_err(), "Should return error for unknown wire type");
    }

    #[test]
    fn test_find_field_overflow() {
        // Tag 2, Wire 2, Length = usize::MAX - 1
        let mut data = vec![0x12];
        // Encode a very large length that might overflow when added to offset
        let large_len = u64::MAX - 5;
        let mut val = large_len;
        while val >= 0x80 {
            data.push((val & 0x7F | 0x80) as u8);
            val >>= 7;
        }
        data.push(val as u8);

        let res = find_field(&data, 2);
        // Should not panic on large length — either returns error or handles gracefully
        assert!(res.is_err(), "Should return error for absurdly large length in find_field");
    }
}
