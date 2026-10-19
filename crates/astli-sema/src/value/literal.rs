//! Literals as written: numbers, sized and not, reals and strings.

use super::bits::{Bit, Bits};

/// What a literal means, before the expression around it sizes it.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Bits(Bits),
    /// `'0`, `'1`, `'x` or `'z`: as wide as its context, every bit the one
    /// written.
    Fill(Bit),
    Real(f64),
    String(Box<[u8]>),
    /// A time, whose value depends on the time unit in force; or a literal
    /// that is malformed, which the lexer or parser reports.
    Unknown,
}

/// The literal `text` spells, its tokens joined without trivia, as an
/// expression's [`ExprKind::Literal`](crate::ExprKind::Literal) holds it.
pub fn literal(text: &str) -> Literal {
    let triple = text.len() >= 6 && text.starts_with(r#"""""#) && text.ends_with(r#"""""#);
    if triple {
        return Literal::String(unescape(&text[3..text.len() - 3]));
    }
    if let Some(quoted) = text.strip_prefix('"') {
        return Literal::String(unescape(quoted.strip_suffix('"').unwrap_or(quoted)));
    }
    if let Some((size, based)) = text.split_once('\'') {
        if size.is_empty() && based.len() == 1 {
            return match based {
                "0" => Literal::Fill(Bit::Zero),
                "1" => Literal::Fill(Bit::One),
                "x" | "X" => Literal::Fill(Bit::X),
                "z" | "Z" => Literal::Fill(Bit::Z),
                _ => Literal::Unknown,
            };
        }
        return based_literal(size, based).map_or(Literal::Unknown, Literal::Bits);
    }
    let digits: String = text.chars().filter(|&c| c != '_').collect();
    if digits.ends_with('s') {
        return Literal::Unknown;
    }
    if digits.contains(['.', 'e', 'E']) {
        return digits.parse().map_or(Literal::Unknown, Literal::Real);
    }
    // Unsized decimal is an `int`, truncated if it does not fit.
    match decimal(&digits, 32) {
        Some(bits) => Literal::Bits(bits.resize(32).with_signed(true)),
        None => Literal::Unknown,
    }
}

/// `size'[s]<base><digits>`, `size` empty when unsized.
fn based_literal(size: &str, based: &str) -> Option<Bits> {
    let size: String = size.chars().filter(|&c| c != '_').collect();
    let width = match size.as_str() {
        "" => None,
        size => match size.parse::<u32>() {
            Ok(0) | Err(_) => return None,
            Ok(width) if width > Bits::MAX_WIDTH => return None,
            Ok(width) => Some(width),
        },
    };
    let (signed, based) = match based.strip_prefix(['s', 'S']) {
        Some(rest) => (true, rest),
        None => (false, based),
    };
    let mut chars = based.chars();
    let base = chars.next()?.to_ascii_lowercase();
    let digits: Vec<char> = chars.filter(|&c| c != '_').collect();
    if digits.is_empty() {
        return None;
    }
    let bits = match base {
        'b' => power_of_two(&digits, 1)?,
        'o' => power_of_two(&digits, 3)?,
        'h' => power_of_two(&digits, 4)?,
        'd' => match digits.as_slice() {
            [digit] if unknown_digit(*digit).is_some() => {
                Bits::filled(1, false, unknown_digit(*digit)?)
            }
            _ => decimal(&digits.iter().collect::<String>(), 1)?,
        },
        _ => return None,
    };
    // Unsized is at least 32 bits, and as many as its value needs.
    let width = width.unwrap_or(bits.needed().max(32));
    // Padding repeats an `x` or `z` leading the digits, and is 0 otherwise.
    let pad = match bits.bit(bits.width() - 1) {
        bit @ (Bit::X | Bit::Z) => bit,
        _ => Bit::Zero,
    };
    let padded = match pad {
        Bit::X | Bit::Z if width > bits.width() => {
            let fill = Bits::filled(width - bits.width(), false, pad);
            Bits::concat([&fill, &bits])
        }
        _ => bits.resize(width),
    };
    Some(padded.with_signed(signed))
}

fn unknown_digit(digit: char) -> Option<Bit> {
    match digit {
        'x' | 'X' => Some(Bit::X),
        'z' | 'Z' | '?' => Some(Bit::Z),
        _ => None,
    }
}

/// Binary, octal or hexadecimal digits, `bits` each, as many bits as they
/// spell.
fn power_of_two(digits: &[char], bits: u32) -> Option<Bits> {
    let width = u32::try_from(digits.len()).ok()?.checked_mul(bits)?;
    if width > Bits::MAX_WIDTH {
        return None;
    }
    let parts = digits.iter().map(|&digit| match unknown_digit(digit) {
        Some(bit) => Some(Bits::filled(bits, false, bit)),
        None => {
            let value = digit.to_digit(1 << bits)?;
            Some(Bits::from_u64(bits, false, u64::from(value)))
        }
    });
    let parts: Vec<Bits> = parts.collect::<Option<_>>()?;
    Some(Bits::concat(&parts))
}

/// Decimal digits, unsigned, at least `width` bits and as many as the value
/// needs.
fn decimal(digits: &str, width: u32) -> Option<Bits> {
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    // Each digit needs at most four bits.
    let bound = u32::try_from(digits.len()).ok()?.checked_mul(4)?.max(width);
    if bound > Bits::MAX_WIDTH {
        return None;
    }
    let ten = Bits::from_u64(bound, false, 10);
    let mut value = Bits::from_u64(bound, false, 0);
    for digit in digits.bytes() {
        let digit = Bits::from_u64(bound, false, u64::from(digit - b'0'));
        value = value.mul(&ten).add(&digit);
    }
    Some(value.resize(value.needed().max(width)))
}

/// A string literal's bytes, its escapes replaced.
fn unescape(text: &str) -> Box<[u8]> {
    let mut out = Vec::with_capacity(text.len());
    let mut bytes = text.bytes().peekable();
    while let Some(byte) = bytes.next() {
        if byte != b'\\' {
            out.push(byte);
            continue;
        }
        let Some(escaped) = bytes.next() else {
            break;
        };
        match escaped {
            b'n' => out.push(b'\n'),
            b't' => out.push(b'\t'),
            b'v' => out.push(0x0b),
            b'f' => out.push(0x0c),
            b'a' => out.push(0x07),
            b'\n' => {}
            b'\r' => {
                bytes.next_if_eq(&b'\n');
            }
            b'x' => {
                let mut value = 0u8;
                for _ in 0..2 {
                    match bytes.peek().and_then(|&b| (b as char).to_digit(16)) {
                        Some(digit) => {
                            value = value * 16 + digit as u8;
                            bytes.next();
                        }
                        None => break,
                    }
                }
                out.push(value);
            }
            b'0'..=b'7' => {
                let mut value = u32::from(escaped - b'0');
                for _ in 0..2 {
                    match bytes.peek() {
                        Some(&b @ b'0'..=b'7') => {
                            value = value * 8 + u32::from(b - b'0');
                            bytes.next();
                        }
                        _ => break,
                    }
                }
                out.push(value as u8);
            }
            other => out.push(other),
        }
    }
    out.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bits(text: &str) -> String {
        match literal(text) {
            Literal::Bits(bits) => bits.to_string(),
            other => format!("{other:?}"),
        }
    }

    #[test]
    fn sized() {
        assert_eq!(bits("8'hFF"), "8'd255");
        assert_eq!(bits("8'sb1111_1111"), "-8'sd1");
        assert_eq!(bits("4'b1x"), "4'b001x");
        assert_eq!(bits("4'bx1"), "4'bxxx1");
        assert_eq!(bits("8'hz"), "8'bz");
        assert_eq!(bits("12'o7"), "12'd7");
        assert_eq!(bits("4'hff"), "4'd15");
        assert_eq!(bits("6'd?"), "6'bz");
        assert_eq!(bits("16'd65535"), "16'd65535");
        assert_eq!(bits("3'd9"), "3'd1");
    }

    #[test]
    fn without_size() {
        assert_eq!(bits("12"), "32'sd12");
        assert_eq!(bits("1_000"), "32'sd1000");
        assert_eq!(bits("4294967295"), "-32'sd1");
        assert_eq!(bits("'hff"), "32'd255");
        assert_eq!(bits("'sd5"), "32'sd5");
        assert_eq!(bits("'hx"), "32'bx");
        assert_eq!(bits("'h1_0000_0000_0"), "37'd68719476736");
        assert_eq!(bits("'h0000_0000_01"), "32'd1");
        assert_eq!(literal("'1"), Literal::Fill(Bit::One));
        assert_eq!(literal("'z"), Literal::Fill(Bit::Z));
    }

    #[test]
    fn others() {
        assert_eq!(literal("1.5"), Literal::Real(1.5));
        assert_eq!(literal("2e3"), Literal::Real(2000.0));
        assert_eq!(literal("10ns"), Literal::Unknown);
        assert_eq!(literal("8'b2"), Literal::Unknown);
        assert_eq!(literal("0'd1"), Literal::Unknown);
        assert_eq!(
            literal(r#""a\tb\x41\101\"""#),
            Literal::String(b"a\tbAA\"".to_vec().into())
        );
        // A triple-quoted string keeps its newline and quotes, and drops an
        // escaped newline as a quoted one does.
        assert_eq!(
            literal("\"\"\"say \"hi\"\nnow \\\nok\"\"\""),
            Literal::String(b"say \"hi\"\nnow ok".to_vec().into())
        );
        assert_eq!(literal(r#""""""""#), Literal::String(Box::default()));
    }
}
