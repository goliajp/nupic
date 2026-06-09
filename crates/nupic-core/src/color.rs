use std::str::FromStr;

use crate::error::Error;

/// 8-bit sRGB color with straight alpha.
///
/// CSS-style color value. Internal image processing may operate in linear
/// floating-point; this is the human-input boundary type.
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const TRANSPARENT: Self = Self::rgba(0, 0, 0, 0);
    pub const BLACK: Self = Self::rgb(0, 0, 0);
    pub const WHITE: Self = Self::rgb(255, 255, 255);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self::rgba(r, g, b, 255)
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }
}

/// Parses CSS-style color strings.
///
/// Accepts `#rgb`, `#rgba`, `#rrggbb`, `#rrggbbaa`, or one of the named
/// colors: `black`, `white`, `transparent` (alias `none`).
impl FromStr for Color {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        match trimmed.to_ascii_lowercase().as_str() {
            "transparent" | "none" => return Ok(Self::TRANSPARENT),
            "black" => return Ok(Self::BLACK),
            "white" => return Ok(Self::WHITE),
            _ => {}
        }
        let hex = trimmed.strip_prefix('#').ok_or_else(|| {
            Error::InvalidColor(format!("expected '#' prefix or named color: {s:?}"))
        })?;
        let bytes = hex.as_bytes();
        let (r, g, b, a) = match bytes.len() {
            3 => (
                expand(hex_nibble(bytes[0])?),
                expand(hex_nibble(bytes[1])?),
                expand(hex_nibble(bytes[2])?),
                255,
            ),
            4 => (
                expand(hex_nibble(bytes[0])?),
                expand(hex_nibble(bytes[1])?),
                expand(hex_nibble(bytes[2])?),
                expand(hex_nibble(bytes[3])?),
            ),
            6 => (
                hex_byte(&bytes[0..2])?,
                hex_byte(&bytes[2..4])?,
                hex_byte(&bytes[4..6])?,
                255,
            ),
            8 => (
                hex_byte(&bytes[0..2])?,
                hex_byte(&bytes[2..4])?,
                hex_byte(&bytes[4..6])?,
                hex_byte(&bytes[6..8])?,
            ),
            n => {
                return Err(Error::InvalidColor(format!(
                    "hex must have 3/4/6/8 digits, got {n}: {s:?}"
                )));
            }
        };
        Ok(Self::rgba(r, g, b, a))
    }
}

fn hex_nibble(c: u8) -> Result<u8, Error> {
    match c {
        b'0'..=b'9' => Ok(c - b'0'),
        b'a'..=b'f' => Ok(c - b'a' + 10),
        b'A'..=b'F' => Ok(c - b'A' + 10),
        _ => Err(Error::InvalidColor(format!(
            "invalid hex digit: {:?}",
            c as char
        ))),
    }
}

fn hex_byte(slice: &[u8]) -> Result<u8, Error> {
    Ok((hex_nibble(slice[0])? << 4) | hex_nibble(slice[1])?)
}

fn expand(nibble: u8) -> u8 {
    (nibble << 4) | nibble
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_named() {
        assert_eq!("black".parse::<Color>().unwrap(), Color::BLACK);
        assert_eq!("BLACK".parse::<Color>().unwrap(), Color::BLACK);
        assert_eq!("white".parse::<Color>().unwrap(), Color::WHITE);
        assert_eq!("transparent".parse::<Color>().unwrap(), Color::TRANSPARENT);
        assert_eq!("none".parse::<Color>().unwrap(), Color::TRANSPARENT);
    }

    #[test]
    fn parse_hex_3_digit() {
        assert_eq!("#f00".parse::<Color>().unwrap(), Color::rgb(255, 0, 0));
        assert_eq!(
            "#abc".parse::<Color>().unwrap(),
            Color::rgb(0xaa, 0xbb, 0xcc)
        );
    }

    #[test]
    fn parse_hex_4_digit() {
        assert_eq!(
            "#f008".parse::<Color>().unwrap(),
            Color::rgba(255, 0, 0, 0x88)
        );
    }

    #[test]
    fn parse_hex_6_digit() {
        assert_eq!(
            "#e5e7eb".parse::<Color>().unwrap(),
            Color::rgb(0xe5, 0xe7, 0xeb)
        );
    }

    #[test]
    fn parse_hex_8_digit() {
        assert_eq!(
            "#e5e7eb80".parse::<Color>().unwrap(),
            Color::rgba(0xe5, 0xe7, 0xeb, 0x80)
        );
    }

    #[test]
    fn parse_rejects_missing_hash() {
        assert!("e5e7eb".parse::<Color>().is_err());
    }

    #[test]
    fn parse_rejects_invalid_digit() {
        assert!("#xyz".parse::<Color>().is_err());
    }

    #[test]
    fn parse_rejects_wrong_length() {
        assert!("#fffff".parse::<Color>().is_err());
        assert!("#f".parse::<Color>().is_err());
    }
}
