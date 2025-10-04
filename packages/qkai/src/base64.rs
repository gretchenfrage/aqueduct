//! Handling ed25519 representation as base64 url-safe strings (also hex).

use crate::error::Error;

const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";

/// Encode ed25519 raw bytes into their representation as base64 ascii characters.
pub(crate) fn base64_encode(bytes: [u8; 32]) -> [u8; 43] {
    let mut output = [0; 43];
    for i in 0..11 {
        // pull in group of up to 3 binary bytes
        let mut grp = [0; 4];
        let grp_offset = i * 3;
        let grp_len = usize::min(3, 32 - grp_offset);
        grp[..grp_len].copy_from_slice(&bytes[grp_offset..grp_offset + grp_len]);
        let mut grp = u32::from_be_bytes(grp);

        // put out group of up to 4 base64 chars
        let output_grp_offset = i * 4;
        let output_grp_len = usize::min(4, 43 - output_grp_offset);
        for j in 0..output_grp_len {
            output[output_grp_offset + j] = ALPHABET[((grp & 0xfc000000) >> 26) as usize];
            grp <<= 6;
        }
    }
    output
}

/// Decode ed25519 raw bytes from their representation as base64 ascii characters.
pub(crate) fn base64_decode(base64: &[u8]) -> Result<[u8; 32], Error> {
    if base64.len() != 43 {
        return Err(Error::Base64WrongLength);
    }
    let mut bytes = [0; 32];
    for i in 0..11 {
        // pull in  group of up to 4 base64 chars
        let mut grp = 0;
        let input_grp_offset = i * 4;
        let input_grp_len = usize::min(4, 43 - input_grp_offset);
        for j in 0..input_grp_len {
            grp |= (base64_decode_char(base64[input_grp_offset + j])? as u32) << (26 - j * 6);
        }
        let grp = u32::to_be_bytes(grp);

        // put out group of up to 3 binary bytes
        let grp_offset = i * 3;
        let grp_len = usize::min(3, 32 - grp_offset);
        bytes[grp_offset..grp_offset + grp_len].copy_from_slice(&grp[..grp_len]);
        for j in grp_len..3 {
            if grp[j] != 0 {
                return Err(Error::Base64ExtraBits);
            }
        }
    }
    Ok(bytes)
}

// convert a single base64 char into a 6-bit integer
fn base64_decode_char(c: u8) -> Result<u8, Error> {
    if c >= b'A' && c <= b'Z' {
        Ok(c - b'A')
    } else if c >= b'a' && c <= b'z' {
        Ok(c - b'a' + 26)
    } else if c >= b'0' && c <= b'9' {
        Ok(c - b'0' + 52)
    } else if c == b'-' {
        Ok(62)
    } else if c == b'_' {
        Ok(63)
    } else {
        Err(Error::Base64IllegalCharacter)
    }
}

/// Encode ed25519 raw bytes into their representation as lower-hex ascii characters.
pub(crate) fn hex_encode(bytes: [u8; 32]) -> [u8; 64] {
    let mut output = [0; 64];
    for (i, &b) in bytes.iter().enumerate() {
        for (j, n) in [(b & 0xf0) >> 4, b & 0x0f].into_iter().enumerate() {
            output[i * 2 + j] = b"0123456789abcdef"[n as usize];
        }
    }
    output
}

/// Decode ed25519 raw bytes from their representation as lower-hex ascii characters.
pub(crate) fn hex_decode(hex: &[u8]) -> Result<[u8; 32], Error> {
    if hex.len() != 64 {
        return Err(Error::HexadecimalWrongLength);
    }
    let mut bytes = [0; 32];
    for (i, b) in bytes.iter_mut().enumerate() {
        *b = (hex_decode_char(hex[i * 2])? << 4) | hex_decode_char(hex[i * 2 + 1])?;
    }
    Ok(bytes)
}

// convert a single hex char into a nibble
fn hex_decode_char(c: u8) -> Result<u8, Error> {
    if c >= b'0' && c <= b'9' {
        Ok(c - b'0')
    } else if c >= b'a' && c <= b'f' {
        Ok(c - b'a' + 10)
    } else {
        Err(Error::HexadecimalIllegalCharacter)
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn foobar() {}
}
