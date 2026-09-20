//-
// Copyright 2018 Jason Lingle
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Demonstrates the `fork` and `timeout` features on a deliberately
//! exponential `fib`.
//!
//! Backs the Proptest Book's `forking` chapter. The `test_fib` property runs
//! the `fib(n) >= n` expectation over arbitrary `u64`, where large `n` runs
//! far too long, overflows the stack, or overflows integer arithmetic; `fork`
//! isolates each case in a subprocess and `timeout` bounds it, so the run
//! survives the crashes and still shrinks. Fails by design.

use std::iter;
use std::process::Termination;
use std::string::FromUtf8Error;

use proptest::num::u64::ANY;
use proptest::strict::ensure_property_with_transport;
use proptest::test_runner::Config;
use proptest::test_runner::PropertyResult;
use proptest::test_runner::PropertyTransport;
use strict_test_support::ConditionFailure;
use strict_test_support::PredicateFailure;
use strict_test_support::ensure_that;

/// The original argument and the complete fallible Fibonacci result.
type Observation = (u64, Option<u64>);
/// The sole assertion context carried by this example's wire format.
const CONTEXT: &str = "the tutorial property expects fib(n) >= n without overflow";

/// Concrete failures of this example's fixed-width wire contract.
#[derive(Debug, Eq, PartialEq, thiserror::Error)]
enum FibonacciTransportError {
  /// The complete rejected frame payload.
  #[error("invalid Fibonacci transport payload: {payload:?}")]
  InvalidPayload {
    /// Complete bytes that failed the wire schema.
    payload: Vec<u8>,
  },
  /// The codec cannot reconstruct an assertion from another context.
  #[error("unsupported Fibonacci assertion context: {context}")]
  UnsupportedContext {
    /// Original context outside this codec's declared domain.
    context: String,
  },
  /// Invalid context bytes retain their native UTF-8 diagnostic.
  #[error("invalid Fibonacci assertion context encoding: {source}")]
  ContextEncoding {
    /// Invalid bytes and the native decoding failure.
    source: FromUtf8Error,
  },
}

/// A fixed-width representation of the case, observation, and assertion result.
struct FibonacciTransport;

impl PropertyTransport<u64, Observation, PredicateFailure<Observation>> for FibonacciTransport {
  type Error = FibonacciTransportError;

  fn encode_case(&mut self, case: &u64) -> Result<Vec<u8>, Self::Error> {
    Ok(case.to_le_bytes().to_vec())
  }

  fn decode_case(&mut self, payload: &[u8]) -> Result<u64, Self::Error> {
    let (&[encoded], &[]) = payload.as_chunks::<8>() else {
      return Err(FibonacciTransportError::InvalidPayload {
        payload: payload.to_vec()
      });
    };
    Ok(u64::from_le_bytes(encoded))
  }

  fn encode(&mut self, case: &u64, result: &Result<Observation, PredicateFailure<Observation>>) -> Result<Vec<u8>, Self::Error> {
    let (observation, failed, condition) = match *result {
      Ok(ref subject) => (subject, false, false),
      Err(ref failure) => {
        if failure.source.context != CONTEXT {
          return Err(FibonacciTransportError::UnsupportedContext {
            context: failure.source.context.to_owned(),
          });
        }
        (&failure.subject, true, failure.source.condition)
      }
    };
    let mut bytes = case.to_le_bytes().to_vec();
    bytes.extend_from_slice(&observation.0.to_le_bytes());
    bytes.extend_from_slice(&observation.1.unwrap_or_default().to_le_bytes());
    bytes.extend_from_slice(&[u8::from(observation.1.is_some()), u8::from(failed), u8::from(condition)]);
    Ok(bytes)
  }

  fn decode(&mut self, payload: &[u8]) -> Result<(u64, Result<Observation, PredicateFailure<Observation>>), Self::Error> {
    let invalid = || FibonacciTransportError::InvalidPayload {
      payload: payload.to_vec()
    };
    let (case, observation_bytes) = payload.split_first_chunk::<8>().ok_or_else(invalid)?;
    let (input, result_bytes) = observation_bytes.split_first_chunk::<8>().ok_or_else(invalid)?;
    let (encoded, tags) = result_bytes.split_first_chunk::<8>().ok_or_else(invalid)?;
    let fibonacci = u64::from_le_bytes(*encoded);
    let (produced, failed, condition) = match *tags {
      [0, failed @ 0..=1, condition @ 0..=1] if fibonacci == 0 => (None, failed, condition),
      [1, failed @ 0..=1, condition @ 0..=1] => (Some(fibonacci), failed, condition),
      _ => return Err(invalid()),
    };
    let subject = (u64::from_le_bytes(*input), produced);
    let result = if failed == 1 {
      Err(PredicateFailure {
        subject,
        source: ConditionFailure {
          condition: condition == 1,
          context:   CONTEXT,
        },
      })
    } else if condition == 0 {
      Ok(subject)
    } else {
      return Err(invalid());
    };
    Ok((u64::from_le_bytes(*case), result))
  }

  fn restore(&mut self, _: &u64, _: &u64) -> Result<(), Self::Error> {
    Ok(())
  }

  fn restore_interrupted(&mut self, _: &u64, _: &u64) -> Result<(), Self::Error> {
    Ok(())
  }

  fn encode_error(&mut self, error: &Self::Error) -> Vec<u8> {
    match *error {
      FibonacciTransportError::InvalidPayload {
        ref payload,
      } => iter::once(0).chain(payload.iter().copied()).collect(),
      FibonacciTransportError::UnsupportedContext {
        ref context,
      } => iter::once(1).chain(context.bytes()).collect(),
      FibonacciTransportError::ContextEncoding {
        ref source,
      } => iter::once(2).chain(source.as_bytes().iter().copied()).collect(),
    }
  }

  fn decode_error(&mut self, payload: &[u8]) -> Result<Self::Error, Self::Error> {
    match *payload {
      [0, ref bytes @ ..] => Ok(FibonacciTransportError::InvalidPayload {
        payload: bytes.to_vec()
      }),
      [1, ref bytes @ ..] => String::from_utf8(bytes.to_vec())
        .map(|context| FibonacciTransportError::UnsupportedContext {
          context,
        })
        .map_err(|source| FibonacciTransportError::ContextEncoding {
          source,
        }),
      [2, ref bytes @ ..] => match String::from_utf8(bytes.to_vec()) {
        Ok(_) => Err(FibonacciTransportError::InvalidPayload {
          payload: payload.to_vec()
        }),
        Err(source) => Ok(FibonacciTransportError::ContextEncoding {
          source,
        }),
      },
      _ => Err(FibonacciTransportError::InvalidPayload {
        payload: payload.to_vec()
      }),
    }
  }
}

/// Calculate `fib(n)` recursively with deliberately exponential work.
fn fib(n: u64) -> Option<u64> {
  if n <= 1 {
    return Some(n);
  }
  let left = fib(n.saturating_sub(1))?;
  let right = fib(n.saturating_sub(2))?;
  left.checked_add(right)
}

/// Run the tutorial property with explicit transport for both returned outcomes.
#[allow(
  clippy::single_call_fn,
  reason = "the named tutorial property separates its explicit fork contract from the executable boundary"
)]
fn test_fib() -> PropertyResult<u64, Observation, PredicateFailure<Observation>, FibonacciTransportError> {
  ensure_property_with_transport(
    &ANY,
    "the tutorial Fibonacci property holds",
    Config {
      // Timeout implies fork; both settings are shown for clarity.
      fork: true,
      timeout: 1000,
      test_name: Some(concat!(module_path!(), "::test_fib")),
      ..Config::default()
    },
    FibonacciTransport,
    |input| {
      ensure_that((input, fib(input)), CONTEXT, |observed| {
        observed.1.is_some_and(|produced| produced >= observed.0)
      })
    },
  )
}

fn main() -> impl Termination {
  test_fib().map(drop)
}

#[cfg(test)]
mod tests {
  use super::*;

  /// A native assertion result supported by the example codec.
  type Returned = Result<Observation, PredicateFailure<Observation>>;
  /// Original values, encoded bytes, and the complete decode attempt.
  type RoundTrip = (
    u64,
    Returned,
    Result<Vec<u8>, FibonacciTransportError>,
    Option<Result<(u64, Returned), FibonacciTransportError>>,
  );
  /// Every observation remains available if the round-trip predicate fails.
  type Check<T> = Result<(), PredicateFailure<T>>;

  #[test]
  fn codec_preserves_success_absence_and_assertion_failures() -> Check<Vec<RoundTrip>> {
    let mut codec = FibonacciTransport;
    let observations = [(0, None), (1, Some(0)), (u64::MAX, Some(u64::MAX))];
    let results = observations.into_iter().flat_map(|subject| {
      [
        Ok(subject),
        Err(PredicateFailure {
          subject,
          source: ConditionFailure {
            condition: false,
            context:   CONTEXT,
          },
        }),
        Err(PredicateFailure {
          subject,
          source: ConditionFailure {
            condition: true,
            context:   CONTEXT,
          },
        }),
      ]
    });
    let round_trips = results
      .map(|original| {
        let case = 17;
        let encoded = codec.encode(&case, &original);
        let decoded = encoded.as_ref().ok().map(|payload| codec.decode(payload));
        (case, original, encoded, decoded)
      })
      .collect::<Vec<_>>();
    ensure_that(
      round_trips,
      "case, optional value, success or failure, condition and context survive transport",
      |observed| {
        observed.iter().all(|&(case, ref original, ref encoded, ref decoded)| {
          encoded.as_ref().is_ok_and(|payload| payload.len() == 27)
            && decoded.as_ref().is_some_and(|result| {
              result
                .as_ref()
                .is_ok_and(|&(restored_case, ref restored)| restored_case == case && restored == original)
            })
        })
      },
    )
    .map(drop)
  }

  /// Encoded input, decoded input, and independently decoded invalid widths.
  type Cases = (
    Vec<(u64, Result<u64, FibonacciTransportError>)>,
    Vec<(Vec<u8>, Result<u64, FibonacciTransportError>)>,
  );

  #[test]
  fn case_codec_accepts_full_width_values_and_rejects_other_lengths() -> Check<Cases> {
    let mut codec = FibonacciTransport;
    let valid = [0, 1, u64::MAX]
      .into_iter()
      .map(|input| {
        let decoded = codec.encode_case(&input).and_then(|payload| codec.decode_case(&payload));
        (input, decoded)
      })
      .collect();
    let invalid = [0, 1, 7, 9, 16]
      .into_iter()
      .map(|length| {
        let payload = vec![0; length];
        let decoded = codec.decode_case(&payload);
        (payload, decoded)
      })
      .collect();
    ensure_that(
      (valid, invalid),
      "cases require exactly eight bytes and retain every rejected payload",
      |observed: &Cases| {
        observed
          .0
          .iter()
          .all(|&(input, ref result)| result.as_ref().is_ok_and(|decoded| *decoded == input))
          && observed.1.iter().all(
            |&(ref input, ref result)| matches!(*result, Err(FibonacciTransportError::InvalidPayload { ref payload }) if payload == input),
          )
      },
    )
    .map(drop)
  }

  /// Malformed evaluation bytes paired with the complete decoder outcome.
  type Malformed = Vec<(Vec<u8>, Result<(u64, Returned), FibonacciTransportError>)>;

  #[test]
  fn evaluation_codec_rejects_truncation_trailing_bytes_and_invalid_tags() -> Check<Malformed> {
    let lengths = (0..27).chain([28]);
    let mut inputs: Vec<_> = lengths.map(|length| vec![0; length]).collect();
    for tags in [[2, 0, 0], [0, 2, 0], [0, 0, 2], [0, 0, 1]] {
      let mut bytes = vec![0; 24];
      bytes.extend(tags);
      inputs.push(bytes);
    }
    let mut nonzero_absence = vec![0; 16];
    nonzero_absence.extend(1_u64.to_le_bytes());
    nonzero_absence.extend([0, 0, 0]);
    inputs.push(nonzero_absence);
    let mut codec = FibonacciTransport;
    let decoded_frames = inputs
      .into_iter()
      .map(|payload| {
        let decoded = codec.decode(&payload);
        (payload, decoded)
      })
      .collect();
    ensure_that(
      decoded_frames,
      "malformed evaluations retain their complete rejected bytes",
      |observed: &Malformed| {
        observed.iter().all(
          |&(ref input, ref result)| matches!(*result, Err(FibonacciTransportError::InvalidPayload { ref payload }) if payload == input),
        )
      },
    )
    .map(drop)
  }

  /// Encoding rejection and every transported codec failure.
  type CodecFailures = (
    Result<Vec<u8>, FibonacciTransportError>,
    Vec<(FibonacciTransportError, Result<FibonacciTransportError, FibonacciTransportError>)>,
  );

  #[test]
  fn codec_rejects_unknown_context_and_round_trips_its_native_errors() -> Check<CodecFailures> {
    let mut codec = FibonacciTransport;
    let rejected = codec.encode(
      &3,
      &Err(PredicateFailure {
        subject: (3, Some(2)),
        source:  ConditionFailure {
          condition: false,
          context:   "another property",
        },
      }),
    );
    let mut errors = vec![
      FibonacciTransportError::InvalidPayload {
        payload: vec![0, 255, 2]
      },
      FibonacciTransportError::UnsupportedContext {
        context: "another property".to_owned(),
      },
    ];
    if let Err(source) = String::from_utf8(vec![255]) {
      errors.push(FibonacciTransportError::ContextEncoding {
        source,
      });
    }
    let decoded_errors = errors
      .into_iter()
      .map(|error| {
        let encoded = codec.encode_error(&error);
        let decoded = codec.decode_error(&encoded);
        (error, decoded)
      })
      .collect();
    ensure_that(
      (rejected, decoded_errors),
      "unsupported contexts fail explicitly and error transport retains native details",
      |observed: &CodecFailures| {
        matches!(observed.0, Err(FibonacciTransportError::UnsupportedContext { ref context }) if context == "another property")
          && observed.1.len() == 3
          && observed
            .1
            .iter()
            .all(|&(ref original, ref decoded)| decoded.as_ref().is_ok_and(|restored| restored == original))
      },
    )
    .map(drop)
  }
}
