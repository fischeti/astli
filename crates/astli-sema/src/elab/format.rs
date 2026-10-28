//! `$sformatf`: the string a format makes of the values it is given, as
//! `$display` would write them.
//!
//! An integer takes as many characters as its width may need unless the
//! format gives a width, `%0d` the fewest; a decimal is padded with spaces,
//! the other bases with zeros. A real is written as C's `printf` writes it.
//! What depends on where the call runs, `%m`'s instance path and `%t`'s
//! time format, is unknown, and so are `%g` and the specifiers that write
//! raw bits or patterns.

use super::expr::{bits_string, string_bits};
use crate::value::{Bit, Bits, Value};

/// A specifier's flags, width and precision.
#[derive(Default)]
struct Options {
    left: bool,
    zero: bool,
    width: Option<usize>,
    precision: Option<usize>,
}

/// `format` with each specifier replaced by the next of `args`; `None` if
/// a specifier is malformed or not modelled, or the arguments do not match.
pub(crate) fn sformatf(format: &[u8], args: &[Value]) -> Option<Box<[u8]>> {
    let mut out = Vec::new();
    let mut args = args.iter();
    let mut rest = format;
    while let Some((&byte, after)) = rest.split_first() {
        rest = after;
        if byte != b'%' {
            out.push(byte);
            continue;
        }
        if let Some(after) = rest.strip_prefix(b"%") {
            out.push(b'%');
            rest = after;
            continue;
        }
        let mut options = Options::default();
        loop {
            match rest.first() {
                Some(b'-') if !options.left => options.left = true,
                Some(b'0') if !options.zero => options.zero = true,
                _ => break,
            }
            rest = &rest[1..];
        }
        options.width = number(&mut rest);
        if let Some(after) = rest.strip_prefix(b".") {
            rest = after;
            options.precision = Some(number(&mut rest).unwrap_or(0));
        }
        let (&specifier, after) = rest.split_first()?;
        rest = after;
        let arg = args.next()?;
        match specifier.to_ascii_lowercase() {
            b'd' => integer(&mut out, arg, 10, options)?,
            b'h' | b'x' => integer(&mut out, arg, 16, options)?,
            b'o' => integer(&mut out, arg, 8, options)?,
            b'b' => integer(&mut out, arg, 2, options)?,
            b'e' | b'f' => real(&mut out, arg, specifier.to_ascii_lowercase(), options)?,
            b's' => {
                let text = match arg {
                    Value::String(bytes) => bytes.clone(),
                    Value::Bits(bits) => bits_string(bits),
                    _ => return None,
                };
                pad(&mut out, &text, options.width, false, b' ');
            }
            b'c' => match arg {
                Value::Bits(bits) => out.push(bits.resize(8).to_u64()? as u8),
                _ => return None,
            },
            _ => return None,
        }
    }
    match args.next() {
        Some(_) => None,
        None => Some(out.into()),
    }
}

/// The decimal number at the start of `rest`, taken from it.
fn number(rest: &mut &[u8]) -> Option<usize> {
    let digits = rest.iter().take_while(|byte| byte.is_ascii_digit()).count();
    let (number, after) = rest.split_at(digits);
    *rest = after;
    std::str::from_utf8(number).ok()?.parse().ok()
}

/// `text` padded with `fill` to `width`, on the right if `left`.
fn pad(out: &mut Vec<u8>, text: &[u8], width: Option<usize>, left: bool, fill: u8) {
    let padding = width.unwrap_or(0).saturating_sub(text.len());
    if !left {
        out.extend(std::iter::repeat_n(fill, padding));
    }
    out.extend_from_slice(text);
    if left {
        out.extend(std::iter::repeat_n(fill, padding));
    }
}

/// `arg` as an integer in `radix`: in decimal signed if it is, `x` or `z`
/// if all its bits are, `X` or `Z` if some are; in the other bases a digit
/// at a time, unsigned.
fn integer(out: &mut Vec<u8>, arg: &Value, radix: u32, options: Options) -> Option<()> {
    let bits = match arg {
        Value::Bits(bits) => bits.clone(),
        Value::Real(real) => Bits::from_f64(64, true, *real),
        Value::String(bytes) => string_bits(bytes),
        _ => return None,
    };
    let width = bits.width();
    let text = match radix {
        10 => decimal(&bits),
        _ => digits(&bits, radix.trailing_zeros()),
    };
    // A `0` flag asks for the fewest characters, unless a width follows.
    let least = match (options.width, options.zero) {
        (Some(width), _) => width,
        (None, true) => 0,
        (None, false) => match radix {
            10 => (f64::from(width) / 10f64.log2()).ceil() as usize + usize::from(bits.is_signed()),
            _ => width.div_ceil(radix.trailing_zeros()) as usize,
        },
    };
    let fill = if radix == 10 { b' ' } else { b'0' };
    pad(out, text.as_bytes(), Some(least), options.left, fill);
    Some(())
}

fn decimal(bits: &Bits) -> String {
    if bits.has_unknown() {
        let all: Vec<Bit> = (0..bits.width()).map(|index| bits.bit(index)).collect();
        return digit(&all).to_string();
    }
    match bits.is_negative() {
        true => format!("-{}", bits.neg().with_signed(false).decimal()),
        false => bits.decimal(),
    }
}

/// `bits` in a base of `per` bits a digit, most significant first, with no
/// leading zeros.
fn digits(bits: &Bits, per: u32) -> String {
    let text: String = (0..bits.width().div_ceil(per))
        .rev()
        .map(|place| {
            let lo = place * per;
            let of: Vec<Bit> = (lo..(lo + per).min(bits.width()))
                .map(|index| bits.bit(index))
                .collect();
            digit(&of)
        })
        .collect();
    match text.trim_start_matches('0') {
        "" => "0".to_string(),
        text => text.to_string(),
    }
}

/// The digit `bits` make, least significant first: `x` or `z` if all of
/// them are, `X` or `Z` if some are.
fn digit(bits: &[Bit]) -> char {
    let all = |of: Bit| bits.iter().all(|&bit| bit == of);
    if all(Bit::X) {
        'x'
    } else if all(Bit::Z) {
        'z'
    } else if bits.contains(&Bit::X) {
        'X'
    } else if bits.contains(&Bit::Z) {
        'Z'
    } else {
        let value =
            (bits.iter().rev()).fold(0, |value, &bit| value * 2 + u32::from(bit == Bit::One));
        char::from_digit(value, 16).unwrap()
    }
}

/// `arg` as a real, `%f` or `%e` as C writes it: six digits after the point
/// unless a precision says otherwise, and an exponent of at least two.
fn real(out: &mut Vec<u8>, arg: &Value, specifier: u8, options: Options) -> Option<()> {
    let real = match arg {
        Value::Real(real) => *real,
        Value::Bits(bits) => bits.to_f64(),
        _ => return None,
    };
    if !real.is_finite() {
        return None;
    }
    let precision = options.precision.unwrap_or(6);
    let text = match specifier {
        b'f' => format!("{real:.precision$}"),
        _ => {
            let text = format!("{real:.precision$e}");
            let (mantissa, exponent) = text.split_once('e')?;
            let exponent: i32 = exponent.parse().ok()?;
            let sign = if exponent < 0 { '-' } else { '+' };
            format!("{mantissa}e{sign}{:02}", exponent.abs())
        }
    };
    match (options.zero && !options.left, text.strip_prefix('-')) {
        // Zeros go after the sign.
        (true, Some(magnitude)) => {
            out.push(b'-');
            let width = options.width.map(|width| width.saturating_sub(1));
            pad(out, magnitude.as_bytes(), width, false, b'0');
        }
        (true, None) => pad(out, text.as_bytes(), options.width, false, b'0'),
        (false, _) => pad(out, text.as_bytes(), options.width, options.left, b' '),
    }
    Some(())
}
