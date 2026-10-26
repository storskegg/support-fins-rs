//! Number <-> string conversions behind `to_js_string`, `to_fixed`, `string_to_number` and
//! `parse_float`. Private helpers of `jsnum`; the public wrappers live in `jsnum.rs`.

// ------------------------------------------------------------ number -> string

/// ECMAScript Number::toString(x) for radix 10 (ES2024 6.1.6.1.20), which V8 implements in
/// `DoubleToCString` (src/numbers/conversions.cc). The shortest round-trip digits come from
/// Rust's `{:e}` formatting; the placement and exponent rules below are the spec's.
pub(super) fn number_to_string(x: f64) -> String {
    if x.is_nan() {
        return "NaN".to_string();
    }
    if x == 0.0 {
        // both zeros, including -0
        return "0".to_string();
    }
    if x.is_infinite() {
        return if x < 0.0 { "-Infinity" } else { "Infinity" }.to_string();
    }
    let mut out = String::new();
    if x < 0.0 {
        out.push('-');
    }
    // "d.ddde-7" / "de21": the shortest digits that round-trip
    let sci = format!("{:e}", x.abs());
    let (mantissa, exp) = sci.split_once('e').expect("{:e} always has an exponent");
    let exp: i32 = exp.parse().expect("{:e} exponent is an integer");
    let digits: String = mantissa.chars().filter(|c| *c != '.').collect();
    let k = digits.len() as i32;
    let n = exp + 1; // the decimal point sits after n digits
    if k <= n && n <= 21 {
        out.push_str(&digits);
        for _ in 0..(n - k) {
            out.push('0');
        }
    } else if 0 < n && n <= 21 {
        out.push_str(&digits[..n as usize]);
        out.push('.');
        out.push_str(&digits[n as usize..]);
    } else if -6 < n && n <= 0 {
        out.push_str("0.");
        for _ in 0..(-n) {
            out.push('0');
        }
        out.push_str(&digits);
    } else {
        let e = n - 1;
        out.push_str(&digits[..1]);
        if k > 1 {
            out.push('.');
            out.push_str(&digits[1..]);
        }
        out.push('e');
        out.push(if e < 0 { '-' } else { '+' });
        out.push_str(&e.abs().to_string());
    }
    out
}

// ------------------------------------------------------------ toFixed

/// Little-endian base-2^32 unsigned integer: just enough arithmetic for `to_fixed`.
struct Big(Vec<u32>);

impl Big {
    fn from_u128(mut v: u128) -> Big {
        let mut limbs = Vec::new();
        while v != 0 {
            limbs.push(v as u32);
            v >>= 32;
        }
        Big(limbs)
    }

    fn mul_small(&mut self, m: u32) {
        let mut carry = 0u64;
        for limb in self.0.iter_mut() {
            let t = *limb as u64 * m as u64 + carry;
            *limb = t as u32;
            carry = t >> 32;
        }
        if carry != 0 {
            self.0.push(carry as u32);
        }
    }

    /// self += 2^bit
    fn add_pow2(&mut self, bit: usize) {
        let idx = bit / 32;
        if self.0.len() <= idx {
            self.0.resize(idx + 1, 0);
        }
        let mut carry = 1u64 << (bit % 32);
        let mut i = idx;
        while carry != 0 {
            if i == self.0.len() {
                self.0.push(0);
            }
            let t = self.0[i] as u64 + carry;
            self.0[i] = t as u32;
            carry = t >> 32;
            i += 1;
        }
    }

    /// self >>= bits (discarding the low bits)
    fn shr(&mut self, bits: usize) {
        let limbs = bits / 32;
        if limbs >= self.0.len() {
            self.0.clear();
            return;
        }
        self.0.drain(..limbs);
        let b = bits % 32;
        if b != 0 {
            let mut carry = 0u32;
            for limb in self.0.iter_mut().rev() {
                let next = *limb << (32 - b);
                *limb = (*limb >> b) | carry;
                carry = next;
            }
        }
        while self.0.last() == Some(&0) {
            self.0.pop();
        }
    }

    fn to_decimal(mut self) -> String {
        if self.0.is_empty() {
            return "0".to_string();
        }
        let mut chunks: Vec<u32> = Vec::new();
        while !self.0.is_empty() {
            let mut rem = 0u64;
            for limb in self.0.iter_mut().rev() {
                let cur = (rem << 32) | *limb as u64;
                *limb = (cur / 1_000_000_000) as u32;
                rem = cur % 1_000_000_000;
            }
            chunks.push(rem as u32);
            while self.0.last() == Some(&0) {
                self.0.pop();
            }
        }
        let mut s = chunks.pop().unwrap().to_string();
        for c in chunks.iter().rev() {
            s.push_str(&format!("{c:09}"));
        }
        s
    }
}

/// Number.prototype.toFixed (ES2024 21.1.3.3). `n` is the integer for which `n / 10^f - x` is
/// closest to zero, the larger one on a tie, taken on the exact binary value of `x`
/// (V8: `DoubleToFixedCString`, src/numbers/conversions.cc).
pub(super) fn number_to_fixed(x: f64, digits: u32) -> String {
    if x.is_nan() {
        return "NaN".to_string();
    }
    if x.abs() >= 1e21 {
        return number_to_string(x);
    }
    let neg = x < 0.0;
    let bits = x.abs().to_bits();
    let exp_field = ((bits >> 52) & 0x7ff) as i32;
    let frac = bits & ((1u64 << 52) - 1);
    // |x| = m * 2^e exactly
    let (m, e) = if exp_field == 0 {
        (frac, -1074)
    } else {
        (frac | (1u64 << 52), exp_field - 1075)
    };
    let n = if m == 0 {
        "0".to_string()
    } else if e >= 0 {
        let mut big = Big::from_u128((m as u128) << e);
        for _ in 0..digits {
            big.mul_small(10);
        }
        big.to_decimal()
    } else {
        // n = floor((m * 10^f + 2^(k-1)) / 2^k): round half up
        let k = (-e) as usize;
        let mut big = Big::from_u128(m as u128);
        for _ in 0..digits {
            big.mul_small(10);
        }
        big.add_pow2(k - 1);
        big.shr(k);
        big.to_decimal()
    };
    let f = digits as usize;
    let mut out = String::new();
    if neg {
        out.push('-');
    }
    if f == 0 {
        out.push_str(&n);
    } else {
        let padded = if n.len() <= f {
            format!("{}{}", "0".repeat(f + 1 - n.len()), n)
        } else {
            n
        };
        let (int_part, frac_part) = padded.split_at(padded.len() - f);
        out.push_str(int_part);
        out.push('.');
        out.push_str(frac_part);
    }
    out
}

// ------------------------------------------------------------ string -> number

/// ECMAScript WhiteSpace and LineTerminator code points (what `Number(s)` trims). Rust's
/// `char::is_whitespace` differs: it has U+0085 and lacks U+FEFF.
fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{9}'
            | '\u{a}'
            | '\u{b}'
            | '\u{c}'
            | '\u{d}'
            | '\u{20}'
            | '\u{a0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200a}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202f}'
            | '\u{205f}'
            | '\u{3000}'
            | '\u{feff}'
    )
}

/// Length of the longest prefix of `s` that is a StrUnsignedDecimalLiteral without `Infinity`
/// (digits, optional `.digits`, optional exponent), or `None` if there is no such prefix.
fn scan_decimal(s: &[u8]) -> Option<usize> {
    let mut i = 0;
    let mut int_digits = 0;
    while i < s.len() && s[i].is_ascii_digit() {
        i += 1;
        int_digits += 1;
    }
    let mut frac_digits = 0;
    if i < s.len() && s[i] == b'.' {
        i += 1;
        while i < s.len() && s[i].is_ascii_digit() {
            i += 1;
            frac_digits += 1;
        }
    }
    if int_digits + frac_digits == 0 {
        return None;
    }
    if i < s.len() && (s[i] == b'e' || s[i] == b'E') {
        let mut j = i + 1;
        if j < s.len() && (s[j] == b'+' || s[j] == b'-') {
            j += 1;
        }
        let digits_start = j;
        while j < s.len() && s[j].is_ascii_digit() {
            j += 1;
        }
        if j > digits_start {
            i = j;
        }
    }
    Some(i)
}

/// Correctly rounded value of a validated decimal literal (optionally signed).
fn parse_decimal(literal: &str) -> f64 {
    // Rust's parse is correctly rounded (round-half-even), as V8's strtod is.
    literal.parse::<f64>().unwrap_or(f64::NAN)
}

/// Value of the digits of a `0x` / `0o` / `0b` literal, rounded to nearest even the way V8's
/// `InternalStringToIntDouble` does. `None` when a digit is invalid or there are none.
fn parse_radix_pow2(digits: &str, bits_per_digit: u32) -> Option<f64> {
    if digits.is_empty() {
        return None;
    }
    // all the bits, most significant first, without leading zeros
    let mut bits: Vec<bool> = Vec::new();
    for c in digits.chars() {
        let d = c.to_digit(1 << bits_per_digit)?;
        for b in (0..bits_per_digit).rev() {
            let bit = (d >> b) & 1 == 1;
            if bit || !bits.is_empty() {
                bits.push(bit);
            }
        }
    }
    if bits.is_empty() {
        return Some(0.0);
    }
    let len = bits.len();
    let take = len.min(53);
    let mut mantissa: u64 = 0;
    for &b in &bits[..take] {
        mantissa = (mantissa << 1) | b as u64;
    }
    let mut shift = (len - take) as i32;
    if len > 53 {
        let round_bit = bits[53];
        let sticky = bits[54..].iter().any(|&b| b);
        if round_bit && (sticky || mantissa & 1 == 1) {
            mantissa += 1; // may become 2^53, which is still exact
        }
    }
    if shift > 1100 {
        return Some(f64::INFINITY);
    }
    if mantissa == 1u64 << 53 {
        mantissa >>= 1;
        shift += 1;
    }
    Some(libm::ldexp(mantissa as f64, shift))
}

/// `Number(s)`: ECMAScript StringToNumber (ES2024 7.1.4.1.1; V8 `StringToDouble`,
/// src/numbers/conversions.cc).
pub(super) fn string_to_number_impl(s: &str) -> f64 {
    let t = s.trim_matches(is_js_whitespace);
    if t.is_empty() {
        return 0.0;
    }
    let b = t.as_bytes();
    if b.len() > 2 && b[0] == b'0' {
        let radix_bits = match b[1] {
            b'x' | b'X' => Some(4),
            b'o' | b'O' => Some(3),
            b'b' | b'B' => Some(1),
            _ => None,
        };
        if let Some(bits) = radix_bits {
            return parse_radix_pow2(&t[2..], bits).unwrap_or(f64::NAN);
        }
    }
    let (negative, body) = match b[0] {
        b'+' => (false, &t[1..]),
        b'-' => (true, &t[1..]),
        _ => (false, t),
    };
    if body == "Infinity" {
        return if negative { f64::NEG_INFINITY } else { f64::INFINITY };
    }
    match scan_decimal(body.as_bytes()) {
        Some(n) if n == body.len() => parse_decimal(t),
        _ => f64::NAN,
    }
}

/// `parseFloat(s)` (ES2024 19.2.4; V8 `StringToDouble` with ALLOW_TRAILING_JUNK).
pub(super) fn parse_float_impl(s: &str) -> f64 {
    let t = s.trim_start_matches(is_js_whitespace);
    let (negative, body, sign_len) = match t.as_bytes().first() {
        Some(b'+') => (false, &t[1..], 1),
        Some(b'-') => (true, &t[1..], 1),
        _ => (false, t, 0),
    };
    if body.starts_with("Infinity") {
        return if negative { f64::NEG_INFINITY } else { f64::INFINITY };
    }
    match scan_decimal(body.as_bytes()) {
        Some(n) => parse_decimal(&t[..sign_len + n]),
        None => f64::NAN,
    }
}
