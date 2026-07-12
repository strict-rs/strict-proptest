//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Arbitrary implementations for `std::io`.

use std::io::BufRead;
use std::io::BufReader;
use std::io::BufWriter;
use std::io::Chain;
use std::io::Cursor;
use std::io::Empty;
use std::io::Error;
use std::io::ErrorKind;
use std::io::ErrorKind::AddrInUse;
use std::io::ErrorKind::AddrNotAvailable;
use std::io::ErrorKind::AlreadyExists;
use std::io::ErrorKind::BrokenPipe;
use std::io::ErrorKind::ConnectionAborted;
use std::io::ErrorKind::ConnectionRefused;
use std::io::ErrorKind::ConnectionReset;
use std::io::ErrorKind::Interrupted;
use std::io::ErrorKind::InvalidData;
use std::io::ErrorKind::InvalidInput;
use std::io::ErrorKind::NotConnected;
use std::io::ErrorKind::NotFound;
use std::io::ErrorKind::Other;
use std::io::ErrorKind::PermissionDenied;
use std::io::ErrorKind::TimedOut;
use std::io::ErrorKind::UnexpectedEof;
use std::io::ErrorKind::WouldBlock;
use std::io::ErrorKind::WriteZero;
use std::io::LineWriter;
use std::io::Lines;
use std::io::Read;
use std::io::Repeat;
use std::io::SeekFrom;
use std::io::Sink;
use std::io::Split;
use std::io::Stderr;
use std::io::Stdin;
use std::io::Stdout;
use std::io::Take;
use std::io::Write;
use std::io::empty;
use std::io::repeat;
use std::io::sink;
use std::io::stderr;
use std::io::stdin;
use std::io::stdout;

use crate::arbitrary::Arbitrary;
use crate::arbitrary::SMapped;
use crate::arbitrary::any;
use crate::arbitrary::arbitrary;
use crate::arbitrary::arbitrary_with;
use crate::strategy::Just;
use crate::strategy::Strategy as _;
use crate::strategy::TupleUnion;
use crate::strategy::Union;
use crate::strategy::WeightedStrategy;
use crate::strategy::statics::static_map;

// TODO: IntoInnerError
// Consider: std::io::Initializer

/// Implements `Arbitrary` (and the matching `lift1!`) for a buffered
/// reader/writer wrapper.
///
/// Given the wrapper type and its inner `Read`/`Write` bound, it generates an
/// arbitrary inner value plus an optional capacity, calling `with_capacity`
/// when a capacity is drawn and `new` otherwise.
macro_rules! buffer {
    ($type: ident, $bound: path) => {
        arbitrary!(
            [A: Arbitrary + $bound] $type<A>,
            SMapped<(A, Option<u16>), Self>, A::Parameters;
            args => static_map(
                arbitrary_with(product_pack![args, Default::default()]),
                |(inner, cap)| {
                    if let Some(cap) = cap {
                        $type::with_capacity(usize::from(cap), inner)
                    } else {
                        $type::new(inner)
                    }
                }
            )
        );

        lift1!([$bound] $type<A>; base =>
            (base, any::<Option<u16>>()).prop_map(|(inner, cap)| {
                if let Some(cap) = cap {
                    $type::with_capacity(usize::from(cap), inner)
                } else {
                    $type::new(inner)
                }
            })
        );
    };
}

buffer!(BufReader, Read);
buffer!(BufWriter, Write);
buffer!(LineWriter, Write);

arbitrary!(
    [A: Read + Arbitrary, B: Read + Arbitrary] Chain<A, B>,
    SMapped<(A, B), Self>, product_type![A::Parameters, B::Parameters];
    args => static_map(arbitrary_with(args), |(first, second)| first.chain(second))
);

std_wrap_ctor_default!(Cursor);

lazy_just!(
      Empty, empty
    ; Sink, sink
    ; Stderr, stderr
    ; Stdin, stdin
    ; Stdout, stdout
);

wrap_ctor!([BufRead] Lines, BufRead::lines);

arbitrary!(Repeat, SMapped<u8, Self>; static_map(any::<u8>(), repeat));

arbitrary!([A: BufRead + Arbitrary] Split<A>,
    SMapped<(A, u8), Self>, A::Parameters;
    args => static_map(
        arbitrary_with(product_pack![args, Default::default()]),
        |(reader, byte)| reader.split(byte)
    )
);
lift1!(['static + BufRead] Split<A>;
    base => (base, any::<u8>()).prop_map(|(reader, byte)| reader.split(byte)));

arbitrary!([A: Read + Arbitrary] Take<A>,
    SMapped<(A, u64), Self>, A::Parameters;
    args => static_map(
        arbitrary_with(product_pack![args, Default::default()]),
        |(reader, limit)| reader.take(limit)
    )
);
lift1!(['static + Read] Take<A>;
    base => (base, any::<u64>()).prop_map(|(reader, limit)| reader.take(limit)));

arbitrary!(ErrorKind, Union<Just<Self>>;
    Union::new(
    [ NotFound
    , PermissionDenied
    , ConnectionRefused
    , ConnectionReset
    , ConnectionAborted
    , NotConnected
    , AddrInUse
    , AddrNotAvailable
    , BrokenPipe
    , AlreadyExists
    , WouldBlock
    , InvalidInput
    , InvalidData
    , TimedOut
    , WriteZero
    , Interrupted
    , Other
    , UnexpectedEof
    // TODO: watch this type for variant-additions.
    ].iter().copied().map(Just))
);

arbitrary!(
    SeekFrom,
    TupleUnion<(
        WeightedStrategy<SMapped<u64, SeekFrom>>,
        WeightedStrategy<SMapped<i64, SeekFrom>>,
        WeightedStrategy<SMapped<i64, SeekFrom>>,
    )>;
    prop_oneof![
        static_map(any::<u64>(), SeekFrom::Start),
        static_map(any::<i64>(), SeekFrom::End),
        static_map(any::<i64>(), SeekFrom::Current)
    ]
);

arbitrary!(Error, SMapped<ErrorKind, Self>;
    static_map(arbitrary(), Error::from)
);

#[cfg(test)]
mod test {
  use super::*;
  use crate::std_facade::Vec;

  no_panic_test!(
      buf_reader  => BufReader<Repeat>,
      buf_writer  => BufWriter<Sink>,
      line_writer => LineWriter<Sink>,
      chain       => Chain<Empty, BufReader<Repeat>>,
      cursor      => Cursor<Empty>,
      empty       => Empty,
      sink        => Sink,
      stderr      => Stderr,
      stdin       => Stdin,
      stdout      => Stdout,
      lines       => Lines<Empty>,
      repeat      => Repeat,
      split       => Split<Cursor<Vec<u8>>>,
      take        => Take<Repeat>,
      error_kind  => ErrorKind,
      seek_from   => SeekFrom,
      error       => Error
  );
}
