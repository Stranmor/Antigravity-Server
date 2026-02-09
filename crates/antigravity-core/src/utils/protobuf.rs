// Protobuf wire format uses fixed bit-width operations:
// - Varint encoding: 7 bits per byte with continuation bit
// - Wire types: 3 bits (0-7), field numbers: remaining bits
// - All casts are bounded by the protocol specification.
#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_lossless,
    clippy::as_conversions,
    clippy::arithmetic_side_effects,
    clippy::indexing_slicing,
    reason = "Protobuf wire format: bit-level operations with protocol-defined bounds"
)]

/// Protobuf Varint encode
pub fn encode_varint(mut value: u64) -> Vec<u8> {
    let mut buf = Vec::new();
    while value >= 0x80 {
        buf.push((value & 0x7F | 0x80) as u8);
        value >>= 7;
    }
    buf.push(value as u8);
    buf
}

/// read Protobuf Varint
pub fn read_varint(data: &[u8], offset: usize) -> Result<(u64, usize), String> {
    let mut result = 0u64;
    let mut shift = 0;
    let mut pos = offset;

    loop {
        if pos >= data.len() {
            return Err("datanotcomplete".to_string());
        }
        let byte = data[pos];
        result |= ((byte & 0x7F) as u64) << shift;
        pos += 1;
        if byte & 0x80 == 0 {
            break;
        }
        shift += 7;
    }

    Ok((result, pos))
}

/// skip Protobuf field
pub fn skip_field(data: &[u8], offset: usize, wire_type: u8) -> Result<usize, String> {
    match wire_type {
        0 => {
            // Varint
            let (_, new_offset) = read_varint(data, offset)?;
            Ok(new_offset)
        },
        1 => {
            // 64-bit
            if offset + 8 > data.len() {
                return Err("data not complete: 64-bit field".to_string());
            }
            Ok(offset + 8)
        },
        2 => {
            // Length-delimited
            let (length, content_offset) = read_varint(data, offset)?;
            let end = content_offset
                .checked_add(length as usize)
                .ok_or_else(|| "overflow in length-delimited field".to_string())?;
            if end > data.len() {
                return Err("data not complete: length-delimited field".to_string());
            }
            Ok(end)
        },
        5 => {
            // 32-bit
            if offset + 4 > data.len() {
                return Err("data not complete: 32-bit field".to_string());
            }
            Ok(offset + 4)
        },
        _ => Err(format!("Unknown wire_type: {}", wire_type)),
    }
}

/// removespecify  Protobuf field
pub fn remove_field(data: &[u8], field_num: u32) -> Result<Vec<u8>, String> {
    let mut result = Vec::new();
    let mut offset = 0;

    while offset < data.len() {
        let start_offset = offset;
        let (tag, new_offset) = read_varint(data, offset)?;
        let wire_type = (tag & 7) as u8;
        let current_field = (tag >> 3) as u32;

        if current_field == field_num {
            // skipthisfield
            offset = skip_field(data, new_offset, wire_type)?;
        } else {
            // preserveotherfield
            let next_offset = skip_field(data, new_offset, wire_type)?;
            if next_offset > data.len() {
                return Err("invalid field offset".to_string());
            }
            result.extend_from_slice(&data[start_offset..next_offset]);
            offset = next_offset;
        }
    }

    Ok(result)
}

/// findspecify  Protobuf fieldcontent (Length-Delimited only)
pub fn find_field(data: &[u8], target_field: u32) -> Result<Option<Vec<u8>>, String> {
    let mut offset = 0;

    while offset < data.len() {
        let (tag, new_offset) = match read_varint(data, offset) {
            Ok(v) => v,
            Err(_) => break, // datanotcomplete，stop
        };

        let wire_type = (tag & 7) as u8;
        let field_num = (tag >> 3) as u32;

        if field_num == target_field && wire_type == 2 {
            let (length, content_offset) = read_varint(data, new_offset)?;
            let end = content_offset + length as usize;
            if end > data.len() {
                return Err("truncated field data".to_string());
            }
            return Ok(Some(data[content_offset..end].to_vec()));
        }

        // skipfield
        offset = skip_field(data, new_offset, wire_type)?;
    }

    Ok(None)
}

/// create OAuthTokenInfo (Field 6)
///
/// struct：
/// message OAuthTokenInfo {
///     optional string access_token = 1;
///     optional string token_type = 2;
///     optional string refresh_token = 3;
///     optional Timestamp expiry = 4;
/// }
pub fn create_oauth_field(access_token: &str, refresh_token: &str, expiry: i64) -> Vec<u8> {
    // Field 1: access_token (string, wire_type = 2)
    let tag1 = (1 << 3) | 2;
    let field1 = {
        let mut f = encode_varint(tag1);
        f.extend(encode_varint(access_token.len() as u64));
        f.extend(access_token.as_bytes());
        f
    };

    // Field 2: token_type (string, fixed value "Bearer", wire_type = 2)
    let tag2 = (2 << 3) | 2;
    let token_type = "Bearer";
    let field2 = {
        let mut f = encode_varint(tag2);
        f.extend(encode_varint(token_type.len() as u64));
        f.extend(token_type.as_bytes());
        f
    };

    // Field 3: refresh_token (string, wire_type = 2)
    let tag3 = (3 << 3) | 2;
    let field3 = {
        let mut f = encode_varint(tag3);
        f.extend(encode_varint(refresh_token.len() as u64));
        f.extend(refresh_token.as_bytes());
        f
    };

    // Field 4: expiry (nested  Timestamp message, wire_type = 2)
    // Timestamp messagecontaining: Field 1: seconds (int64, wire_type = 0)
    let timestamp_tag = 1 << 3; // Field 1, varint (wire_type 0)
    let timestamp_msg = {
        let mut m = encode_varint(timestamp_tag);
        m.extend(encode_varint(expiry as u64));
        m
    };

    let tag4 = (4 << 3) | 2; // Field 4, length-delimited
    let field4 = {
        let mut f = encode_varint(tag4);
        f.extend(encode_varint(timestamp_msg.len() as u64));
        f.extend(timestamp_msg);
        f
    };

    // mergeallfieldas OAuthTokenInfo message
    let oauth_info = [field1, field2, field3, field4].concat();

    // wrapas Field 6 (length-delimited)
    let tag6 = (6 << 3) | 2;
    let mut field6 = encode_varint(tag6);
    field6.extend(encode_varint(oauth_info.len() as u64));
    field6.extend(oauth_info);

    field6
}
