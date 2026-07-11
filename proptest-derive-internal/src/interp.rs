// Copyright 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use syn::{self, BinOp, Expr, Lit, UnOp};

/// Adapted from <https://docs.rs/syn/0.14.2/src/syn/lit.rs.html#943> to accept
/// u128.
#[allow(
    clippy::single_call_fn,
    reason = "read the digit string of an integer literal into a u128 magnitude"
)]
fn parse_lit_int(mut digits: &str) -> Option<u128> {
    /// Get the byte at offset idx, or a default of `b'\0'` if we're looking
    /// past the end of the input buffer.
    pub(crate) fn byte<S: AsRef<[u8]> + ?Sized>(source: &S, idx: usize) -> u8 {
        let bytes = source.as_ref();
        bytes.get(idx).copied().unwrap_or(0)
    }

    let base = match (byte(digits, 0), byte(digits, 1)) {
        (b'0', b'x') => {
            digits = digits.strip_prefix("0x")?;
            16
        }
        (b'0', b'o') => {
            digits = digits.strip_prefix("0o")?;
            8
        }
        (b'0', b'b') => {
            digits = digits.strip_prefix("0b")?;
            2
        }
        (b'0'..=b'9', _) => 10,
        _ => return None,
    };

    let mut magnitude = 0_u128;
    loop {
        let current_byte = byte(digits, 0);
        let digit = match current_byte {
            b'0'..=b'9' => u128::from(current_byte.wrapping_sub(b'0')),
            b'a'..=b'f' if base > 10 => 10_u128
                .saturating_add(u128::from(current_byte.wrapping_sub(b'a'))),
            b'A'..=b'F' if base > 10 => 10_u128
                .saturating_add(u128::from(current_byte.wrapping_sub(b'A'))),
            b'_' => {
                digits = digits.get(1..)?;
                continue;
            }
            // NOTE: Looking at a floating point literal, we don't want to
            // consider these integers.
            b'.' | b'e' | b'E' if base == 10 => return None,
            _ => break,
        };

        if digit >= base {
            return None;
        }

        magnitude = magnitude.checked_mul(base)?.checked_add(digit)?;
        digits = digits.get(1..)?;
    }

    Some(magnitude)
}

/// Parse a suffix of an integer literal.
#[allow(
    clippy::single_call_fn,
    reason = "detect which numeric type suffix an integer literal string carries"
)]
fn parse_suffix(lit: &str) -> Option<&'static str> {
    [
        "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64",
        "u128", "usize",
    ]
    .iter()
    .find(|suffix| lit.ends_with(*suffix))
    .copied()
}

/// Interprets an integer literal in a string.
fn eval_str_int(lit: &str) -> Option<u128> {
    let parsed = parse_lit_int(lit)?;
    let checked_val = if let Some(suffix) = parse_suffix(lit) {
        match suffix {
            "i8" if fits_in_u128(parsed, i8::MAX) => parsed,
            "i16" if fits_in_u128(parsed, i16::MAX) => parsed,
            "i32" if fits_in_u128(parsed, i32::MAX) => parsed,
            "i64" if fits_in_u128(parsed, i64::MAX) => parsed,
            "u8" if parsed <= u128::from(u8::MAX) => parsed,
            "u16" if parsed <= u128::from(u16::MAX) => parsed,
            "u32" if parsed <= u128::from(u32::MAX) => parsed,
            "u64" if parsed <= u128::from(u64::MAX) => parsed,
            "usize" if fits_in_u128(parsed, usize::MAX) => parsed,
            "isize" if fits_in_u128(parsed, isize::MAX) => parsed,
            "u128" => parsed,
            "i128" if fits_in_u128(parsed, i128::MAX) => parsed,

            // Does not fit in suffix:
            _ => return None,
        }
    } else {
        parsed
    };

    Some(checked_val)
}

/// Return whether a parsed unsigned literal fits below a typed maximum.
fn fits_in_u128<T>(parsed: u128, max: T) -> bool
where
    u128: TryFrom<T>,
{
    u128::try_from(max).is_ok_and(|typed_max| parsed <= typed_max)
}

/// Interprets an integer literal.
#[allow(
    clippy::single_call_fn,
    reason = "reduce a syn LitInt token to its constant u128 value"
)]
fn eval_lit_int(lit: &syn::LitInt) -> Option<u128> {
    let literal_text = lit.to_string();
    eval_str_int(&literal_text)
}

/// Interprets a verbatim literal.
#[allow(
    clippy::single_call_fn,
    reason = "resolve a verbatim proc-macro2 literal to a constant u128"
)]
fn eval_lit_verbatim(lit: &proc_macro2::Literal) -> Option<u128> {
    let literal_text = lit.to_string();
    eval_str_int(&literal_text)
}

/// Interprets a literal.
#[allow(
    clippy::single_call_fn,
    reason = "dispatch a literal expression to the int, byte, or verbatim evaluator"
)]
fn eval_lit(lit: &syn::ExprLit) -> Option<u128> {
    match lit.lit {
        Lit::Int(ref int_lit) => eval_lit_int(int_lit),
        Lit::Byte(ref byte_lit) => Some(u128::from(byte_lit.value())),
        Lit::Verbatim(ref verbatim_lit) => eval_lit_verbatim(verbatim_lit),
        Lit::Str(_)
        | Lit::ByteStr(_)
        | Lit::CStr(_)
        | Lit::Char(_)
        | Lit::Float(_)
        | Lit::Bool(_)
        | _ => None,
    }
}

/// Interprets a binary operator on two expressions.
#[allow(
    clippy::single_call_fn,
    reason = "evaluate a checked binary arithmetic or bitwise node"
)]
fn eval_binary(bin: &syn::ExprBinary) -> Option<u128> {
    let lhs = eval_expr(&bin.left)?;
    let rhs = eval_expr(&bin.right)?;
    Some(match bin.op {
        BinOp::Add(_) => lhs.checked_add(rhs)?,
        BinOp::Sub(_) => lhs.checked_sub(rhs)?,
        BinOp::Mul(_) => lhs.checked_mul(rhs)?,
        BinOp::Div(_) => lhs.checked_div(rhs)?,
        BinOp::Rem(_) => lhs.checked_rem(rhs)?,
        BinOp::BitXor(_) => lhs ^ rhs,
        BinOp::BitAnd(_) => lhs & rhs,
        BinOp::BitOr(_) => lhs | rhs,
        BinOp::Shl(_) => lhs.checked_shl(u32::try_from(rhs).ok()?)?,
        BinOp::Shr(_) => lhs.checked_shr(u32::try_from(rhs).ok()?)?,
        BinOp::And(_)
        | BinOp::Or(_)
        | BinOp::Eq(_)
        | BinOp::Lt(_)
        | BinOp::Le(_)
        | BinOp::Ne(_)
        | BinOp::Ge(_)
        | BinOp::Gt(_)
        | BinOp::AddAssign(_)
        | BinOp::SubAssign(_)
        | BinOp::MulAssign(_)
        | BinOp::DivAssign(_)
        | BinOp::RemAssign(_)
        | BinOp::BitXorAssign(_)
        | BinOp::BitAndAssign(_)
        | BinOp::BitOrAssign(_)
        | BinOp::ShlAssign(_)
        | BinOp::ShrAssign(_)
        | _ => return None,
    })
}

/// Interprets unary operator on an expression.
#[allow(
    clippy::single_call_fn,
    reason = "apply the bitwise-not unary operator in the const interpreter"
)]
fn eval_unary(expr: &syn::ExprUnary) -> Option<u128> {
    if let UnOp::Not(_) = expr.op {
        Some(!eval_expr(&expr.expr)?)
    } else {
        None
    }
}

/// A **very** simple CTFE interpreter for some basic arithmetic:
pub(crate) fn eval_expr(expr: &Expr) -> Option<u128> {
    match *expr {
        Expr::Lit(ref lit_expr) => eval_lit(lit_expr),
        Expr::Binary(ref binary_expr) => eval_binary(binary_expr),
        Expr::Unary(ref unary_expr) => eval_unary(unary_expr),
        Expr::Paren(ref paren_expr) => eval_expr(&paren_expr.expr),
        Expr::Group(ref group_expr) => eval_expr(&group_expr.expr),
        Expr::Array(_)
        | Expr::Assign(_)
        | Expr::Async(_)
        | Expr::Await(_)
        | Expr::Block(_)
        | Expr::Break(_)
        | Expr::Call(_)
        | Expr::Cast(_)
        | Expr::Closure(_)
        | Expr::Const(_)
        | Expr::Continue(_)
        | Expr::Field(_)
        | Expr::ForLoop(_)
        | Expr::If(_)
        | Expr::Index(_)
        | Expr::Infer(_)
        | Expr::Let(_)
        | Expr::Loop(_)
        | Expr::Macro(_)
        | Expr::Match(_)
        | Expr::MethodCall(_)
        | Expr::Path(_)
        | Expr::Range(_)
        | Expr::RawAddr(_)
        | Expr::Reference(_)
        | Expr::Repeat(_)
        | Expr::Return(_)
        | Expr::Struct(_)
        | Expr::Try(_)
        | Expr::TryBlock(_)
        | Expr::Tuple(_)
        | Expr::Unsafe(_)
        | Expr::Verbatim(_)
        | Expr::While(_)
        | Expr::Yield(_)
        | _ => None,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn eval(
        expr: &str,
    ) -> Result<Option<u128>, ::strict_test_support::TestFailure> {
        use syn::parse_str;
        let parsed = ::strict_test_support::ensure_ok(
            parse_str(expr),
            "the test case parses as a valid expression",
        )?;
        Ok(eval_expr(&parsed))
    }

    // `Option<u128>` has no `Display`, so the comparison flows through
    // `ensure` rather than `ensure_eq`.
    macro_rules! test {
        ($($name: ident, $case: expr => $result:expr;)*) => {$(
            #[test]
            fn $name(
            ) -> ::core::result::Result<(), ::strict_test_support::TestFailure>
            {
                ::strict_test_support::ensure(
                    eval($case)? == $result,
                    "the interpreted value matches the expected evaluation",
                )
            }
        )*};
    }

    test! {
        accept_lit_bare, "1" => Some(1);
        accept_lit_bare_max, "340282366920938463463374607431768211455"
            => Some(340_282_366_920_938_463_463_374_607_431_768_211_455);
        reject_lit_bare_overflow, "340282366920938463463374607431768211456" => None;
        accept_lit_u8_max, "255u8" => Some(255);
        accept_lit_u16_max, "65535u16" => Some(65535);
        accept_lit_u32_max, "4294967295u32" => Some(4_294_967_295);
        accept_lit_u64_max, "18446744073709551615u64" => Some(18_446_744_073_709_551_615);
        accept_lit_u128_max, "340282366920938463463374607431768211455u128"
            => Some(340_282_366_920_938_463_463_374_607_431_768_211_455);
        reject_lit_u8_overflow, "256u8" => None;
        reject_lit_u16_overflow, "65536u16" => None;
        reject_lit_u32_overflow, "4294967296u32" => None;
        reject_lit_u64_overflow, "18446744073709551616u64" => None;
        reject_lit_u128_overflow, "340282366920938463463374607431768211456u128" => None;
        accept_lit_i8_max, "127i8" => Some(127);
        accept_lit_i16_max, "32767i16" => Some(32767);
        accept_lit_i32_max, "2147483647i32" => Some(2_147_483_647);
        accept_lit_i64_max, "9223372036854775807i64" => Some(9_223_372_036_854_775_807);
        accept_lit_i128_max, "170141183460469231731687303715884105727i128"
            => Some(170_141_183_460_469_231_731_687_303_715_884_105_727);
        reject_lit_i8_overflow, "128i8" => None;
        reject_lit_i16_overflow, "32768i16" => None;
        reject_lit_i32_overflow, "2147483648i32" => None;
        reject_lit_i64_overflow, "9223372036854775808i64" => None;
        reject_lit_i128_overflow, "170141183460469231731687303715884105728i128" => None;
        accept_lit_usize, "42usize" => Some(42);
        accept_lit_isize, "42isize" => Some(42);
        accept_lit_byte, "b'0'" => Some(48);
        reject_lit_negative, "-42" => None;
        accept_add_10_20, "10 + 20" => Some(30);
        accept_add_10u8_20u16, "10u8 + 20u16" => Some(30);
        reject_add_overflow, "340282366920938463463374607431768211456u128 + 1" => None;
        accept_add_commutes, "20 + 10" => Some(30);
        accept_add_5_numbers, "(10 + 20) + 30 + (40 + 50)" => Some(150);
        accept_add_10_0, "10 + 0" => Some(10);
        accept_sub_20_10, "20 - 10" => Some(10);
        reject_sub_10_20, "10 - 20" => None;
        reject_sub_10_11, "10 - 11" => None;
        accept_sub_10_10, "10 - 10" => Some(0);
        accept_mul_42_0, "42 * 0" => Some(0);
        accept_mul_0_42, "0 * 42" => Some(0);
        accept_mul_42_1, "42 * 1" => Some(42);
        accept_mul_1_42, "1 * 42" => Some(42);
        accept_mul_3_4, "3 * 4" => Some(12);
        accept_mul_4_3, "4 * 3" => Some(12);
        accept_mul_1_2_3_4_5, "(1 * 2) * 3 * (4 * 5)" => Some(120);
        reject_div_with_0, "10 / 0" => None;
        accept_div_42_1, "42 / 1" => Some(42);
        accept_div_42_42, "42 / 42" => Some(1);
        accept_div_20_10, "20 / 10" => Some(2);
        accept_div_10_20, "10 / 20" => Some(0);
        reject_rem_with_0, "10 % 0" => None;
        accept_rem_0_4, "0 % 4" => Some(0);
        accept_rem_4_4, "4 % 4" => Some(0);
        accept_rem_8_4, "8 % 4" => Some(0);
        accept_rem_1_4, "1 % 4" => Some(1);
        accept_rem_5_4, "5 % 4" => Some(1);
        accept_rem_2_4, "2 % 4" => Some(2);
        accept_rem_3_4, "3 % 4" => Some(3);
        accept_xor_1, "0b0000 ^ 0b1111" => Some(0b1111);
        accept_xor_2, "0b1111 ^ 0b0000" => Some(0b1111);
        accept_xor_3, "0b1111 ^ 0b1111" => Some(0b0000);
        accept_xor_4, "0b0000 ^ 0b0000" => Some(0b0000);
        accept_xor_5, "0b1100 ^ 0b0011" => Some(0b1111);
        accept_xor_6, "0b1001 ^ 0b1111" => Some(0b0110);
        accept_and_1, "0b0000 & 0b0000" => Some(0b0000);
        accept_and_2, "0b1001 & 0b0101" => Some(0b0001);
        accept_and_3, "0b1111 & 0b1111" => Some(0b1111);
        accept_or_1, "0b0000 | 0b0000" => Some(0b0000);
        accept_or_2, "0b1001 | 0b0101" => Some(0b1101);
        accept_or_3, "0b1111 | 0b1111" => Some(0b1111);
        accept_shl, "0b001000 << 2" => Some(0b10_0000);
        accept_shr, "0b001000 >> 2" => Some(0b00_0010);
        accept_shl_zero, "0b001000 << 0" => Some(0b00_1000);
        accept_shr_zero, "0b001000 >> 0" => Some(0b00_1000);
        reject_shl_rhs_not_u32, "0b001000 << 4294967296" => None;
        reject_shl_overflow, "0b001000 << 429496" => None;
        reject_shr_rhs_not_u32, "0b001000 >> 4294967296" => None;
        reject_shr_underflow, "0b001000 >> 429496" => None;
        accept_complex_arith, "(3 + 4 * 2 - 5) / 6" => Some(1);
    }
}
