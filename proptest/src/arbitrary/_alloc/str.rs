//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Arbitrary implementations for `std::str`.

use crate::std_facade::Vec;
use core::str::{ParseBoolError, Utf8Error, from_utf8};

use crate::arbitrary::*;
use crate::strategy::statics::static_map;
use crate::strategy::*;

arbitrary!(ParseBoolError; "".parse::<bool>().unwrap_err());

/// One weighted arm of the `gen_el_seqs` union: a `Just` of a fixed
/// invalid-UTF-8 tail byte slice.
type ELSeq = WA<Just<&'static [u8]>>;
/// The union over the four candidate invalid-UTF-8 tail sequences that
/// `gen_el_seqs` picks between.
type ELSeqs = TupleUnion<(ELSeq, ELSeq, ELSeq, ELSeq)>;

/// Builds a strategy that picks one of four byte sequences forming an invalid
/// UTF-8 tail, with `error_len` of `None`, `Some(1)`, `Some(2)`, or `Some(3)`
/// respectively; used to construct arbitrary `Utf8Error` values.
#[allow(
    clippy::single_call_fn,
    reason = "the four-arm invalid UTF-8 tail sequence union feeding the Utf8Error strategy"
)]
fn gen_el_seqs() -> ELSeqs {
    prop_oneof![
        Just(&[0xC2]),                   // None
        Just(&[0x80]),                   // Some(1)
        Just(&[0xE0, 0xA0, 0x00]),       // Some(2)
        Just(&[0xF0, 0x90, 0x80, 0x00])  // Some(3)
    ]
}

arbitrary!(Utf8Error, SFnPtrMap<(StrategyFor<u16>, ELSeqs), Utf8Error>;
    static_map((any::<u16>(), gen_el_seqs()), |(vut, elseq)| {
        let bytes = core::iter::repeat_n(b'_', vut as usize)
                    .chain(elseq.iter().cloned())
                    .collect::<Vec<u8>>();
        from_utf8(&bytes).unwrap_err()
    })
);

#[cfg(test)]
mod test {
    use super::*;

    no_panic_test!(
        parse_bool_error => ParseBoolError,
        utf8_error => Utf8Error
    );
}
