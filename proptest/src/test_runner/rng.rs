//-
// Copyright 2017, 2018, 2019, 2020, 2026 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use core::convert::Infallible;
use core::error::Error;
use core::fmt;
use core::result::Result;
use core::str;

use rand::Rng as _;
use rand::RngExt as _;
#[cfg(feature = "std")]
use rand::SeedableRng;
#[cfg(not(feature = "std"))]
use rand::SeedableRng as _;
use rand::TryRng;
#[cfg(feature = "std")]
use rand::rand_core::UnwrapErr;
#[cfg(feature = "std")]
use rand::rngs::SysRng;
use rand_chacha::ChaChaRng;
use rand_xorshift::XorShiftRng;

use crate::std_facade::Arc;
use crate::std_facade::String;
use crate::std_facade::ToOwned as _;
use crate::std_facade::Vec;
use crate::std_facade::format;
use crate::std_facade::vec;
use crate::test_runner::config;

/// Identifies a particular RNG algorithm supported by proptest.
///
/// Proptest supports dynamic configuration of algorithms to allow it to
/// continue operating with persisted regression files and to allow the
/// configuration to be expressed in the `Config` struct.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum RngAlgorithm {
  /// The [XorShift](https://rust-random.github.io/rand/rand_xorshift/struct.XorShiftRng.html)
  /// algorithm. This was the default up through and including Proptest 0.9.0.
  ///
  /// It is faster than `ChaCha` but produces lower quality randomness and has
  /// some pathological cases where it may fail to produce outputs that are
  /// random even to casual observation.
  ///
  /// The seed must be exactly 16 bytes.
  XorShift,
  /// The [ChaCha](https://rust-random.github.io/rand/rand_chacha/struct.ChaChaRng.html)
  /// algorithm. This became the default with Proptest 0.9.1.
  ///
  /// The seed must be exactly 32 bytes.
  #[default]
  ChaCha,
  /// This is not an actual RNG algorithm, but instead returns data directly
  /// from its "seed".
  ///
  /// This is useful when Proptest is being driven from some other entropy
  /// source, such as a fuzzer.
  ///
  /// If the seed is depleted, the RNG will return 0s forever.
  ///
  /// Note that in cases where a new RNG is to be derived from an existing
  /// one, *the data is split evenly between them*, regardless of how much
  /// entropy is actually needed. This means that combinators like
  /// `prop_perturb` and `prop_flat_map` can require extremely large inputs.
  PassThrough,
  /// This is equivalent to the `ChaCha` RNG, with the addition that it
  /// records the bytes used to create a value.
  ///
  /// This is useful when Proptest is used for fuzzing, and a corpus of
  /// initial inputs need to be created. Note that in these cases, you need
  /// to use the `TestRunner` API directly yourself instead of using the
  /// `proptest!` macro, as otherwise there is no way to obtain the bytes
  /// this captures.
  Recorder,
}

impl RngAlgorithm {
  /// The short key identifying this algorithm in the persistence and
  /// replay wire formats (`xs` / `cc` / `pt` / `rc`).
  pub(crate) const fn persistence_key(self) -> &'static str {
    match self {
      Self::XorShift => "xs",
      Self::ChaCha => "cc",
      Self::PassThrough => "pt",
      Self::Recorder => "rc",
    }
  }

  /// The inverse of `persistence_key`: map a wire key back to its
  /// algorithm, or `None` if the key is unrecognized.
  pub(crate) fn from_persistence_key(key: &str) -> Option<Self> {
    match key {
      "xs" => Some(Self::XorShift),
      "cc" => Some(Self::ChaCha),
      "pt" => Some(Self::PassThrough),
      "rc" => Some(Self::Recorder),
      _ => None,
    }
  }
}

// These two are only used for parsing the environment variable
// PROPTEST_RNG_ALGORITHM.
impl str::FromStr for RngAlgorithm {
  type Err = ();
  fn from_str(s: &str) -> Result<Self, ()> {
    Self::from_persistence_key(s).ok_or(())
  }
}
impl fmt::Display for RngAlgorithm {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "{}", self.persistence_key())
  }
}

/// Proptest's random number generator.
#[derive(Clone, Debug)]
pub struct TestRng {
  /// The active algorithm-specific generator state.
  rng: TestRngImpl,
}

/// The algorithm-specific backing state behind a `TestRng`.
#[derive(Clone, Debug)]
enum TestRngImpl {
  /// A `XorShift` generator.
  XorShift(XorShiftRng),
  /// A `ChaCha` generator.
  ChaCha(ChaChaRng),
  /// Raw bytes replayed as "randomness", reading zeros once spent.
  PassThrough {
    /// Offset of the next unread byte in `data`.
    off:  usize,
    /// One past the last byte this generator may read.
    end:  usize,
    /// The shared byte buffer, split across derived generators.
    data: Arc<[u8]>,
  },
  /// A `ChaCha` generator that also records the bytes it emits.
  Recorder {
    /// The underlying `ChaCha` generator.
    rng:    ChaChaRng,
    /// Every byte emitted so far, retrievable via `bytes_used`.
    record: Vec<u8>,
  },
}

/// Seed a fresh RNG of type `R` from OS entropy (`SysRng`).
#[cfg(feature = "std")]
#[allow(
  clippy::single_call_fn,
  reason = "seed a fresh RNG type from OS entropy via the SysRng source"
)]
fn from_sys_rng<R: SeedableRng>() -> R {
  let mut rng = UnwrapErr(SysRng);
  R::from_rng(&mut rng)
}

/// Resolve the configured seed into a seedable RNG: OS entropy for
/// `Random`, a deterministic `seed_from_u64` stream for `Fixed`.
#[cfg(feature = "std")]
fn seeded_or_sys_rng<R: SeedableRng>(rng_seed: config::RngSeed) -> R {
  match rng_seed {
    config::RngSeed::Random => from_sys_rng::<R>(),
    config::RngSeed::Fixed(fixed_seed) => R::seed_from_u64(fixed_seed),
  }
}

#[cfg(any(
  test,
  all(
    not(feature = "std"),
    any(target_arch = "x86", target_arch = "x86_64"),
    feature = "hardware-rng"
  )
))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EntropySeedStatus {
  Filled,
  Fallback,
}

#[cfg(any(
  test,
  all(
    not(feature = "std"),
    any(target_arch = "x86", target_arch = "x86_64"),
    feature = "hardware-rng"
  )
))]
fn fill_entropy_seed<const N: usize, E>(mut seed: [u8; N], fill: impl FnOnce(&mut [u8]) -> Result<(), E>) -> ([u8; N], EntropySeedStatus) {
  match fill(&mut seed) {
    Ok(()) => (seed, EntropySeedStatus::Filled),
    Err(_) => (seed, EntropySeedStatus::Fallback),
  }
}

#[cfg(all(
  not(feature = "std"),
  any(target_arch = "x86", target_arch = "x86_64"),
  feature = "hardware-rng"
))]
fn hardware_seed<const N: usize>(fallback: [u8; N]) -> [u8; N] {
  let (seed, _status) = fill_entropy_seed(fallback, getrandom::fill);
  seed
}

impl TryRng for TestRng {
  type Error = Infallible;

  fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
    Ok(self.next_u32_inner())
  }

  fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
    Ok(self.next_u64_inner())
  }

  fn try_fill_bytes(&mut self, dest: &mut [u8]) -> Result<(), Self::Error> {
    self.fill_bytes_inner(dest);
    Ok(())
  }
}

/// The persisted, algorithm-tagged form of a `TestRng`'s seed.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Seed {
  /// A 16-byte `XorShift` seed.
  XorShift([u8; 16]),
  /// A 32-byte `ChaCha` seed.
  ChaCha([u8; 32]),
  /// A `PassThrough` byte buffer with an optional consumed window.
  PassThrough(Option<(usize, usize)>, Arc<[u8]>),
  /// A 32-byte `Recorder` (`ChaCha`) seed.
  Recorder([u8; 32]),
}

/// Length mismatch between an RNG algorithm's required seed size and the
/// bytes supplied to construct a [`Seed`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SeedLengthError {
  /// The algorithm name for the message (e.g. `"XorShift"`).
  algorithm: &'static str,
  /// The exact seed length that algorithm requires, in bytes.
  required:  usize,
  /// The seed length supplied by the caller, in bytes.
  actual:    usize,
}

impl fmt::Display for SeedLengthError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(
      f,
      "{} requires a {}-byte seed, got {} bytes",
      self.algorithm, self.required, self.actual
    )
  }
}

impl Error for SeedLengthError {}

impl Seed {
  /// Normalize `source` into a fixed-length seed by copying its prefix and
  /// zero-filling any missing suffix.
  fn normalized_seed<const N: usize>(source: &[u8]) -> [u8; N] {
    let mut seed = [0_u8; N];
    for (dst, src) in seed.iter_mut().zip(source) {
      *dst = *src;
    }
    seed
  }

  /// Build a `Seed` for `algorithm` from `seed`, normalizing fixed-length
  /// algorithms by truncating long byte slices and zero-padding short ones.
  /// Use [`Seed::try_from_bytes`] when exact-length validation is required.
  #[allow(
    clippy::single_call_fn,
    reason = "build a Seed from raw bytes by deterministic normalization for infallible construction"
  )]
  pub(crate) fn from_bytes(algorithm: RngAlgorithm, seed: &[u8]) -> Self {
    match algorithm {
      RngAlgorithm::XorShift => Self::XorShift(Self::normalized_seed(seed)),
      RngAlgorithm::ChaCha => Self::ChaCha(Self::normalized_seed(seed)),
      RngAlgorithm::PassThrough => Self::PassThrough(None, seed.into()),
      RngAlgorithm::Recorder => Self::Recorder(Self::normalized_seed(seed)),
    }
  }

  /// Build a `Seed` for `algorithm` from `seed`, returning a
  /// `SeedLengthError` when the fixed-length algorithms are handed the
  /// wrong number of bytes (`PassThrough` accepts any length).
  #[allow(
    clippy::single_call_fn,
    reason = "parse raw bytes into a Seed, returning a typed length error instead of panicking"
  )]
  fn try_from_bytes(algorithm: RngAlgorithm, seed: &[u8]) -> Result<Self, SeedLengthError> {
    fn exact_seed<const N: usize>(algorithm: &'static str, seed: &[u8]) -> Result<[u8; N], SeedLengthError> {
      if seed.len() != N {
        return Err(SeedLengthError {
          algorithm,
          required: N,
          actual: seed.len(),
        });
      }

      let mut output = [0_u8; N];
      for (slot, byte) in output.iter_mut().zip(seed.iter().copied()) {
        *slot = byte;
      }
      Ok(output)
    }

    match algorithm {
      RngAlgorithm::XorShift => exact_seed("XorShift", seed).map(Seed::XorShift),

      RngAlgorithm::ChaCha => exact_seed("ChaCha", seed).map(Seed::ChaCha),

      RngAlgorithm::PassThrough => Ok(Self::PassThrough(None, seed.into())),

      RngAlgorithm::Recorder => exact_seed("Recorder", seed).map(Seed::Recorder),
    }
  }

  /// Decode a `Seed` from one persistence/replay line, or `None` if
  /// the algorithm key is unknown or its payload is malformed.
  #[allow(
    clippy::single_call_fn,
    reason = "name the persistence-line parser used by the failure-persistence boundary"
  )]
  pub(crate) fn from_persistence(string: &str) -> Option<Self> {
    fn from_base16(dst: &mut [u8], src: &str) -> Option<()> {
      if dst.len().saturating_mul(2) != src.len() {
        return None;
      }

      let (src_pairs, _) = src.as_bytes().as_chunks::<2>();
      for (dst_byte, src_pair) in dst.iter_mut().zip(src_pairs) {
        *dst_byte = u8::from_str_radix(str::from_utf8(src_pair).ok()?, 16).ok()?;
      }

      Some(())
    }

    let parts = string.trim().split(char::is_whitespace).collect::<Vec<_>>();
    let (key, fields) = parts.split_first()?;
    RngAlgorithm::from_persistence_key(key).and_then(|alg| match alg {
      RngAlgorithm::XorShift => {
        if 4 != fields.len() {
          return None;
        }

        let mut dwords = [0_u32; 4];
        for (dword, part) in dwords.iter_mut().zip(fields) {
          *dword = part.parse().ok()?;
        }

        let mut seed = [0_u8; 16];
        let (seed_chunks, _) = seed.as_chunks_mut::<4>();
        for (chunk, dword) in seed_chunks.iter_mut().zip(dwords) {
          *chunk = dword.to_le_bytes();
        }
        Some(Self::XorShift(seed))
      }

      RngAlgorithm::ChaCha => {
        let &[payload] = fields else { return None };
        let mut seed = [0_u8; 32];
        from_base16(&mut seed, payload)?;
        Some(Self::ChaCha(seed))
      }

      RngAlgorithm::PassThrough => match *fields {
        [] => Some(Self::PassThrough(None, vec![].into())),
        [payload] => {
          let mut seed = vec![0_u8; payload.len().div_euclid(2)];
          from_base16(&mut seed, payload)?;
          Some(Self::PassThrough(None, seed.into()))
        }
        _ => None,
      },

      RngAlgorithm::Recorder => {
        let &[payload] = fields else { return None };
        let mut seed = [0_u8; 32];
        from_base16(&mut seed, payload)?;
        Some(Self::Recorder(seed))
      }
    })
  }

  /// Encode this `Seed` as its single-line persistence/replay form
  /// (the inverse of `from_persistence`).
  pub(crate) fn to_persistence(&self) -> String {
    const fn hex_digit(nibble: u8) -> char {
      match nibble {
        0 => '0',
        1 => '1',
        2 => '2',
        3 => '3',
        4 => '4',
        5 => '5',
        6 => '6',
        7 => '7',
        8 => '8',
        9 => '9',
        10 => 'a',
        11 => 'b',
        12 => 'c',
        13 => 'd',
        14 => 'e',
        15 => 'f',
        _ => '?',
      }
    }

    fn to_base16(dst: &mut String, src: &[u8]) {
      for &byte in src {
        dst.push(hex_digit(byte >> 4));
        dst.push(hex_digit(byte & 0x0f));
      }
    }

    match *self {
      Self::XorShift(ref seed) => {
        let mut dwords = [0_u32; 4];
        let (seed_chunks, _) = seed.as_chunks::<4>();
        for (dword, chunk) in dwords.iter_mut().zip(seed_chunks) {
          *dword = u32::from_le_bytes(*chunk);
        }
        let [d0, d1, d2, d3] = dwords;
        format!("{} {} {} {} {}", RngAlgorithm::XorShift.persistence_key(), d0, d1, d2, d3)
      }

      Self::ChaCha(ref seed) => {
        let mut string = RngAlgorithm::ChaCha.persistence_key().to_owned();
        string.push(' ');
        to_base16(&mut string, seed);
        string
      }

      Self::PassThrough(bounds, ref bytes) => {
        // An inconsistent window (impossible via the tracked
        // consumption bounds) serializes as the exhausted seed.
        let consumed_bytes = match bounds {
          Some((start, end)) => bytes.get(start..end).unwrap_or(&[]),
          None => bytes.as_ref(),
        };
        let mut string = RngAlgorithm::PassThrough.persistence_key().to_owned();
        string.push(' ');
        to_base16(&mut string, consumed_bytes);
        string
      }

      Self::Recorder(ref seed) => {
        let mut string = RngAlgorithm::Recorder.persistence_key().to_owned();
        string.push(' ');
        to_base16(&mut string, seed);
        string
      }
    }
  }
}

impl TestRng {
  /// Draw the next `u32`, dispatching to the active generator (and
  /// recording it under `Recorder`).
  fn next_u32_inner(&mut self) -> u32 {
    match self.rng {
      TestRngImpl::XorShift(ref mut rng) => rng.next_u32(),
      TestRngImpl::ChaCha(ref mut rng) => rng.next_u32(),
      TestRngImpl::PassThrough {
        ..
      } => {
        let mut buf = [0; 4];
        self.fill_bytes_inner(&mut buf[..]);
        u32::from_le_bytes(buf)
      }
      TestRngImpl::Recorder {
        ref mut rng,
        ref mut record,
      } => {
        let read = rng.next_u32();
        record.extend_from_slice(&read.to_le_bytes());
        read
      }
    }
  }

  /// Draw the next `u64`, dispatching to the active generator (and
  /// recording it under `Recorder`).
  fn next_u64_inner(&mut self) -> u64 {
    match self.rng {
      TestRngImpl::XorShift(ref mut rng) => rng.next_u64(),
      TestRngImpl::ChaCha(ref mut rng) => rng.next_u64(),
      TestRngImpl::PassThrough {
        ..
      } => {
        let mut buf = [0; 8];
        self.fill_bytes_inner(&mut buf[..]);
        u64::from_le_bytes(buf)
      }
      TestRngImpl::Recorder {
        ref mut rng,
        ref mut record,
      } => {
        let read = rng.next_u64();
        record.extend_from_slice(&read.to_le_bytes());
        read
      }
    }
  }

  /// Fill `dest` from the active generator: real randomness for the
  /// algorithmic variants, the remaining window then zeros for
  /// `PassThrough`, recording the bytes under `Recorder`.
  fn fill_bytes_inner(&mut self, dest: &mut [u8]) {
    match self.rng {
      TestRngImpl::XorShift(ref mut rng) => rng.fill_bytes(dest),
      TestRngImpl::ChaCha(ref mut rng) => rng.fill_bytes(dest),
      TestRngImpl::PassThrough {
        ref mut off,
        ref end,
        data: ref bytes,
      } => {
        // Copy as much of the remaining window as fits in `dest`;
        // everything past the window (including the whole of `dest`
        // if the window is exhausted or inconsistent) reads as zero,
        // which is PassThrough's documented depletion behavior.
        let available = bytes.get(*off..*end).unwrap_or(&[]);
        let mut copied = 0_usize;
        for (dst_byte, src_byte) in dest.iter_mut().zip(available) {
          *dst_byte = *src_byte;
          copied = copied.saturating_add(1);
        }
        *off = off.saturating_add(copied);
        for byte in dest.iter_mut().skip(copied) {
          *byte = 0;
        }
      }
      TestRngImpl::Recorder {
        ref mut rng,
        ref mut record,
      } => {
        rng.fill_bytes(dest);
        record.extend_from_slice(dest);
      }
    }
  }

  /// Create a new RNG with the given algorithm and seed.
  ///
  /// Any RNG created with the same algorithm-seed pair will produce the same
  /// sequence of values on all systems and all supporting versions of
  /// proptest.
  ///
  /// Fixed-length algorithms normalize `seed` by truncating long byte slices
  /// and zero-padding short ones. Use [`TestRng::try_from_seed`] when callers
  /// need exact-length validation.
  #[must_use]
  pub fn from_seed(algorithm: RngAlgorithm, seed: &[u8]) -> Self {
    Self::from_seed_internal(Seed::from_bytes(algorithm, seed))
  }

  /// Create a new RNG with the given algorithm and exact-length seed.
  ///
  /// ## Errors
  ///
  /// Returns [`SeedLengthError`] when `seed` does not have the exact length
  /// required by `algorithm`. `RngAlgorithm::PassThrough` accepts any length.
  pub fn try_from_seed(algorithm: RngAlgorithm, seed: &[u8]) -> Result<Self, SeedLengthError> {
    Seed::try_from_bytes(algorithm, seed).map(Self::from_seed_internal)
  }

  /// Dumps the bytes obtained from the RNG so far (only works if the RNG is
  /// set to `Recorder`).
  #[must_use]
  pub fn bytes_used(&self) -> Option<Vec<u8>> {
    match self.rng {
      TestRngImpl::Recorder {
        ref record, ..
      } => Some(record.clone()),
      TestRngImpl::XorShift(_)
      | TestRngImpl::ChaCha(_)
      | TestRngImpl::PassThrough {
        ..
      } => None,
    }
  }

  /// Construct a default `TestRng` from entropy.
  #[allow(
    clippy::single_call_fn,
    reason = "the default TestRng resolved from a configured seed and RNG algorithm"
  )]
  pub(crate) fn default_rng(seed: config::RngSeed, algorithm: RngAlgorithm) -> Self {
    #[cfg(feature = "std")]
    {
      Self {
        rng: match algorithm {
          RngAlgorithm::XorShift => TestRngImpl::XorShift(seeded_or_sys_rng(seed)),
          RngAlgorithm::ChaCha => TestRngImpl::ChaCha(seeded_or_sys_rng(seed)),
          RngAlgorithm::PassThrough => TestRngImpl::PassThrough {
            off:  0,
            end:  0,
            data: vec![].into(),
          },
          RngAlgorithm::Recorder => TestRngImpl::Recorder {
            rng:    seeded_or_sys_rng(seed),
            record: Vec::new(),
          },
        },
      }
    }
    #[cfg(all(
      not(feature = "std"),
      any(target_arch = "x86", target_arch = "x86_64"),
      feature = "hardware-rng"
    ))]
    {
      let _: config::RngSeed = seed;
      Self::hardware_rng(algorithm)
    }
    #[cfg(all(
      not(feature = "std"),
      not(all(any(target_arch = "x86", target_arch = "x86_64"), feature = "hardware-rng"))
    ))]
    {
      let _: config::RngSeed = seed;
      Self::deterministic_rng(algorithm)
    }
  }

  /// The fixed 16-byte seed behind the deterministic `XorShift` RNG.
  const SEED_FOR_XOR_SHIFT: [u8; 16] = [
    0xf4, 0x16, 0x16, 0x48, 0xc3, 0xac, 0x77, 0xac, 0x72, 0x20, 0x0b, 0xea, 0x99, 0x67, 0x2d, 0x6d,
  ];

  /// The fixed 32-byte seed behind the deterministic `ChaCha` (and
  /// `Recorder`) RNG.
  const SEED_FOR_CHA_CHA: [u8; 32] = [
    0xf4, 0x16, 0x16, 0x48, 0xc3, 0xac, 0x77, 0xac, 0x72, 0x20, 0x0b, 0xea, 0x99, 0x67, 0x2d, 0x6d, 0xca, 0x9f, 0x76, 0xaf, 0x1b, 0x09,
    0x73, 0xa0, 0x59, 0x22, 0x6d, 0xc5, 0x46, 0x39, 0x1c, 0x4a,
  ];

  /// Returns a `TestRng` with a seed generated from `getrandom::fill`.
  ///
  /// This is useful in `no_std` scenarios on x86/x86_64 where consumers can
  /// select an entropy backend. OS-less targets that require RDRAND should
  /// build with `--cfg getrandom_backend="rdrand"`.
  #[cfg(all(
    not(feature = "std"),
    any(target_arch = "x86", target_arch = "x86_64"),
    feature = "hardware-rng"
  ))]
  pub fn hardware_rng(algorithm: RngAlgorithm) -> Self {
    Self::from_seed_internal(match algorithm {
      RngAlgorithm::XorShift => Seed::XorShift(hardware_seed(TestRng::SEED_FOR_XOR_SHIFT)),
      RngAlgorithm::ChaCha => Seed::ChaCha(hardware_seed(TestRng::SEED_FOR_CHA_CHA)),
      RngAlgorithm::PassThrough => Seed::PassThrough(None, vec![].into()),
      RngAlgorithm::Recorder => Seed::Recorder(hardware_seed(TestRng::SEED_FOR_CHA_CHA)),
    })
  }

  /// Returns a `TestRng` with a particular hard-coded seed.
  ///
  /// The seed value will always be the same for a particular version of
  /// Proptest and algorithm, but may change across releases.
  ///
  /// This is useful for testing things like strategy implementations without
  /// risking getting "unlucky" RNGs which deviate from average behaviour
  /// enough to cause spurious failures. For example, a strategy for `bool`
  /// which is supposed to produce `true` 50% of the time might have a test
  /// which checks that the distribution is "close enough" to 50%. If every
  /// test run starts with a different RNG, occasionally there will be
  /// spurious test failures when the RNG happens to produce a very skewed
  /// distribution. Using this or `TestRunner::deterministic()` avoids such
  /// issues.
  #[allow(
    clippy::single_call_fn,
    reason = "a fixed-seed TestRng for reproducible strategy and distribution tests"
  )]
  #[must_use]
  pub fn deterministic_rng(algorithm: RngAlgorithm) -> Self {
    Self::from_seed_internal(match algorithm {
      RngAlgorithm::XorShift => Seed::XorShift(Self::SEED_FOR_XOR_SHIFT),
      RngAlgorithm::ChaCha => Seed::ChaCha(Self::SEED_FOR_CHA_CHA),
      RngAlgorithm::PassThrough => Seed::PassThrough(None, vec![].into()),
      RngAlgorithm::Recorder => Seed::Recorder(Self::SEED_FOR_CHA_CHA),
    })
  }

  /// Construct a `TestRng` by the perturbed randomized seed
  /// from an existing `TestRng`.
  pub(crate) fn gen_rng(&mut self) -> Self {
    Self::from_seed_internal(self.new_rng_seed())
  }

  /// Overwrite the given `TestRng` with the provided seed.
  pub(crate) fn set_seed(&mut self, seed: Seed) {
    *self = Self::from_seed_internal(seed);
  }

  /// Generate a new randomized seed, set it to this `TestRng`,
  /// and return the seed.
  pub(crate) fn gen_get_seed(&mut self) -> Seed {
    let seed = self.new_rng_seed();
    self.set_seed(seed.clone());
    seed
  }

  /// Randomize a perturbed randomized seed from the given `TestRng`.
  pub(crate) fn new_rng_seed(&mut self) -> Seed {
    match self.rng {
      TestRngImpl::XorShift(ref mut rng) => {
        let mut seed = rng.random::<[u8; 16]>();

        // Directly using XorShiftRng::from_seed() at this point would
        // result in rng and the returned value being exactly the same.
        // Perturb the seed with some arbitrary values to prevent this.
        let (words, _) = seed.as_chunks_mut::<4>();
        for &mut [ref mut b0, ref mut b1, ref mut b2, ref mut b3] in words {
          *b3 ^= 0xde;
          *b2 ^= 0xad;
          *b1 ^= 0xbe;
          *b0 ^= 0xef;
        }

        Seed::XorShift(seed)
      }

      TestRngImpl::ChaCha(ref mut rng) => Seed::ChaCha(rng.random()),

      TestRngImpl::PassThrough {
        ref mut off,
        ref mut end,
        data: ref bytes,
      } => {
        let len = end.saturating_sub(*off);
        let child_start = off.saturating_add(len.div_euclid(2));
        let child_end = off.saturating_add(len);
        *end = child_start;
        Seed::PassThrough(Some((child_start, child_end)), Arc::clone(bytes))
      }

      TestRngImpl::Recorder {
        ref mut rng, ..
      } => Seed::Recorder(rng.random()),
    }
  }

  /// Construct a `TestRng` from a given seed.
  fn from_seed_internal(seed: Seed) -> Self {
    Self {
      rng: match seed {
        Seed::XorShift(xorshift_seed) => TestRngImpl::XorShift(XorShiftRng::from_seed(xorshift_seed)),

        Seed::ChaCha(chacha_seed) => TestRngImpl::ChaCha(ChaChaRng::from_seed(chacha_seed)),

        Seed::PassThrough(bounds, bytes) => {
          let (start, end) = bounds.unwrap_or((0, bytes.len()));
          TestRngImpl::PassThrough {
            off: start,
            end,
            data: bytes,
          }
        }

        Seed::Recorder(recorder_seed) => TestRngImpl::Recorder {
          rng:    ChaChaRng::from_seed(recorder_seed),
          record: Vec::new(),
        },
      },
    }
  }
}

#[cfg(test)]
mod test {
  use std::borrow::ToOwned as _;
  use std::string::ToString as _;

  use rand::Rng as _;
  #[cfg(feature = "strict-test")]
  use rand::RngExt as _;
  use strict_test_support::TestFailure;
  use strict_test_support::ensure;
  use strict_test_support::ensure_all;
  use strict_test_support::ensure_eq;
  use strict_test_support::ensure_some;

  use super::RngAlgorithm;
  use super::Seed;
  use super::TestRng;
  #[cfg(feature = "strict-test")]
  use crate::arbitrary::any;
  use crate::std_facade::Vec;
  #[cfg(feature = "strict-test")]
  use crate::std_facade::vec;
  #[cfg(feature = "strict-test")]
  use crate::strategy::*;
  #[cfg(feature = "strict-test")]
  use crate::strict::ensure_property;

  #[cfg(feature = "strict-test")]
  #[test]
  fn gen_parse_seeds() -> Result<(), TestFailure> {
    let seeds = prop_oneof![
      any::<[u8; 16]>().prop_map(Seed::XorShift),
      any::<[u8; 32]>().prop_map(Seed::ChaCha),
      any::<Vec<u8>>().prop_map(|raw| Seed::PassThrough(None, raw.into())),
      any::<[u8; 32]>().prop_map(Seed::Recorder),
    ];
    ensure_property(&seeds, "every seed round-trips through the persistence codec", |seed| {
      let parsed = ensure_some(Seed::from_persistence(&seed.to_persistence()), "a persisted seed parses back")?;
      ensure(seed == parsed, "the parsed seed equals the original")
    })
  }

  #[test]
  fn entropy_seed_fill_replaces_fallback_on_success() -> Result<(), TestFailure> {
    let fallback = [1_u8, 2, 3, 4];
    let (seed, status) = super::fill_entropy_seed(fallback, |dest| {
      for (slot, value) in dest.iter_mut().zip([9_u8, 8, 7, 6]) {
        *slot = value;
      }
      Ok::<(), ()>(())
    });

    ensure(status == super::EntropySeedStatus::Filled, "successful entropy fill reports Filled")?;
    ensure(seed == [9, 8, 7, 6], "successful entropy fill replaces the fallback seed")
  }

  #[test]
  fn entropy_seed_fill_keeps_fallback_on_error() -> Result<(), TestFailure> {
    let fallback = [1_u8, 2, 3, 4];
    let (seed, status) = super::fill_entropy_seed(fallback, |_dest| Err::<(), ()>(()));

    ensure(status == super::EntropySeedStatus::Fallback, "failed entropy fill reports Fallback")?;
    ensure(
      seed == [1, 2, 3, 4],
      "failed entropy fill preserves the deterministic fallback seed",
    )
  }

  #[cfg(feature = "strict-test")]
  #[test]
  fn rngs_dont_clone_self_on_genrng() -> Result<(), TestFailure> {
    let seeds = prop_oneof![
      any::<[u8; 16]>().prop_map(Seed::XorShift),
      any::<[u8; 32]>().prop_map(Seed::ChaCha),
      Just(()).prop_perturb(|(), mut rng| {
        let mut buf = vec![0_u8; 2048];
        rng.fill_bytes(&mut buf);
        Seed::PassThrough(None, buf.into())
      }),
      any::<[u8; 32]>().prop_map(Seed::Recorder),
    ];
    ensure_property(&seeds, "derived rngs never repeat their parent's stream", |seed| {
      type Value = [u8; 32];
      let orig = TestRng::from_seed_internal(seed);

      let mut parent_clone = orig.clone();
      let mut first_child = parent_clone.gen_rng();
      ensure(
        parent_clone.random::<Value>() != first_child.random::<Value>(),
        "a child rng differs from its parent",
      )?;

      let mut rng1 = orig;
      let mut rng2 = rng1.gen_rng();
      let mut rng3 = rng1.gen_rng();
      let mut rng4 = rng2.gen_rng();
      let parent = rng1.random::<Value>();
      let child = rng2.random::<Value>();
      let sibling = rng3.random::<Value>();
      let grandchild = rng4.random::<Value>();
      ensure_all(&[
        (parent != child, "first child differs from the parent"),
        (parent != sibling, "second child differs from the parent"),
        (parent != grandchild, "grandchild differs from the parent"),
        (child != sibling, "siblings differ from each other"),
        (child != grandchild, "grandchild differs from its parent's sibling"),
        (sibling != grandchild, "second sibling differs from the grandchild"),
      ])
    })
  }

  #[test]
  fn passthrough_rng_behaves_properly() -> Result<(), TestFailure> {
    let mut rng = TestRng::from_seed(RngAlgorithm::PassThrough, &[
      0xDE, 0xC0, 0x12, 0x34, 0x56, 0x78, 0xFE, 0xCA, 0xEF, 0xBE, 0xAD, 0xDE, 0x01, 0x02, 0x03,
    ]);

    ensure_eq(
      &0x3412_C0DE_u32,
      &rng.next_u32(),
      "the first dword replays the buffer little-endian",
    )?;
    ensure_eq(&0xDEAD_BEEF_CAFE_7856_u64, &rng.next_u64(), "the next qword continues the buffer")?;

    let mut buf = [0_u8; 4];
    rng.fill_bytes(&mut buf[0..4]);
    ensure([1, 2, 3, 0] == buf, "fill_bytes drains the tail and zero-pads")?;
    rng.fill_bytes(&mut buf[0..4]);
    ensure([0, 0, 0, 0] == buf, "a depleted buffer yields zeros")
  }

  #[test]
  fn seeded_xorshift_output_is_stable() -> Result<(), TestFailure> {
    let seed = [
      0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    ];
    let mut rng_u32 = TestRng::from_seed(RngAlgorithm::XorShift, &seed);
    let mut rng_u64 = TestRng::from_seed(RngAlgorithm::XorShift, &seed);
    let mut rng_fill = TestRng::from_seed(RngAlgorithm::XorShift, &seed);

    ensure(
      [471_271_404, 722_341_711, 1_880_555_887, 252_576_780]
        == [rng_u32.next_u32(), rng_u32.next_u32(), rng_u32.next_u32(), rng_u32.next_u32()],
      "the seeded xorshift u32 stream is stable",
    )?;
    ensure(
      [
        3_102_434_025_752_954_860, 1_084_809_011_709_542_767, 17_342_619_095_589_341_798, 5_127_465_042_768_897_837,
      ] == [rng_u64.next_u64(), rng_u64.next_u64(), rng_u64.next_u64(), rng_u64.next_u64()],
      "the seeded xorshift u64 stream is stable",
    )?;

    let mut fill = [0_u8; 16];
    rng_fill.fill_bytes(&mut fill);
    ensure(
      [236, 7, 23, 28, 79, 15, 14, 43, 111, 1, 23, 112, 12, 4, 14, 15] == fill,
      "the seeded xorshift fill_bytes output is stable",
    )
  }

  #[test]
  fn seeded_chacha_output_is_stable() -> Result<(), TestFailure> {
    let seed = [
      0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15,
      0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
    ];
    let mut rng_u32 = TestRng::from_seed(RngAlgorithm::ChaCha, &seed);
    let mut rng_u64 = TestRng::from_seed(RngAlgorithm::ChaCha, &seed);
    let mut rng_fill = TestRng::from_seed(RngAlgorithm::ChaCha, &seed);

    ensure(
      [2_100_034_873, 1_780_073_945, 1_996_733_837, 1_229_642_936]
        == [rng_u32.next_u32(), rng_u32.next_u32(), rng_u32.next_u32(), rng_u32.next_u32()],
      "the seeded chacha u32 stream is stable",
    )?;
    ensure(
      [
        7_645_359_380_336_737_593, 5_281_276_197_874_154_893, 14_729_830_432_180_286_858, 10_530_800_043_416_210_610,
      ] == [rng_u64.next_u64(), rng_u64.next_u64(), rng_u64.next_u64(), rng_u64.next_u64()],
      "the seeded chacha u64 stream is stable",
    )?;

    let mut fill = [0_u8; 16];
    rng_fill.fill_bytes(&mut fill);
    ensure(
      [57, 253, 43, 125, 217, 197, 25, 106, 141, 189, 3, 119, 184, 220, 74, 73] == fill,
      "the seeded chacha fill_bytes output is stable",
    )
  }

  #[test]
  fn derived_child_rng_output_is_stable() -> Result<(), TestFailure> {
    let seed = [
      0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15,
      0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
    ];
    let mut parent = TestRng::from_seed(RngAlgorithm::ChaCha, &seed);
    let mut child = parent.gen_rng();

    ensure(
      [357_635_273, 1_295_757_006, 1_334_659_017, 3_423_482_104]
        == [child.next_u32(), child.next_u32(), child.next_u32(), child.next_u32()],
      "the derived child rng stream is stable",
    )
  }

  #[test]
  fn recorder_bytes_used_matches_emitted_bytes() -> Result<(), TestFailure> {
    let seed = [
      0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15,
      0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
    ];
    let mut rng = TestRng::from_seed(RngAlgorithm::Recorder, &seed);
    let first = rng.next_u32();
    let second = rng.next_u64();
    let mut fill = [0_u8; 16];
    rng.fill_bytes(&mut fill);

    let mut expected = Vec::new();
    expected.extend_from_slice(&first.to_le_bytes());
    expected.extend_from_slice(&second.to_le_bytes());
    expected.extend_from_slice(&fill);

    let bytes_used = ensure_some(rng.bytes_used(), "recorder exposes emitted bytes")?;
    ensure(expected == bytes_used, "the recorder replays exactly the bytes it emitted")
  }

  #[test]
  fn try_from_bytes_accepts_correct_seed_lengths() -> Result<(), TestFailure> {
    ensure_all(&[
      (
        Seed::try_from_bytes(RngAlgorithm::XorShift, &[0; 16]).is_ok(),
        "a 16-byte XorShift seed is accepted",
      ),
      (
        Seed::try_from_bytes(RngAlgorithm::ChaCha, &[0; 32]).is_ok(),
        "a 32-byte ChaCha seed is accepted",
      ),
      (
        Seed::try_from_bytes(RngAlgorithm::Recorder, &[0; 32]).is_ok(),
        "a 32-byte Recorder seed is accepted",
      ),
      (
        Seed::try_from_bytes(RngAlgorithm::PassThrough, &[]).is_ok(),
        "PassThrough accepts any seed length, including empty",
      ),
    ])
  }

  #[test]
  fn try_from_bytes_rejects_wrong_seed_lengths() -> Result<(), TestFailure> {
    let xorshift = Seed::try_from_bytes(RngAlgorithm::XorShift, &[0; 15]).err();
    let chacha = Seed::try_from_bytes(RngAlgorithm::ChaCha, &[0; 31]).err();
    let recorder = Seed::try_from_bytes(RngAlgorithm::Recorder, &[0; 33]).err();
    let render = |error: super::SeedLengthError| error.to_string();
    ensure_eq(
      &"XorShift requires a 16-byte seed, got 15 bytes".to_owned(),
      &ensure_some(xorshift, "a 15-byte XorShift seed is rejected").map(render)?,
      "the XorShift length error reports the required and actual sizes",
    )?;
    ensure_eq(
      &"ChaCha requires a 32-byte seed, got 31 bytes".to_owned(),
      &ensure_some(chacha, "a 31-byte ChaCha seed is rejected").map(render)?,
      "the ChaCha length error reports the required and actual sizes",
    )?;
    ensure_eq(
      &"Recorder requires a 32-byte seed, got 33 bytes".to_owned(),
      &ensure_some(recorder, "a 33-byte Recorder seed is rejected").map(render)?,
      "the Recorder length error reports the required and actual sizes",
    )
  }

  #[test]
  fn persistence_parser_rejects_malformed_payloads() -> Result<(), TestFailure> {
    ensure_all(&[
      (
        Seed::from_persistence("cc abc").is_none(),
        "an odd-length base16 payload is rejected",
      ),
      (Seed::from_persistence("cc").is_none(), "a missing ChaCha payload is rejected"),
      (
        Seed::from_persistence("xs 1 2 3").is_none(),
        "a short XorShift dword list is rejected",
      ),
      (Seed::from_persistence("").is_none(), "an empty line parses to no seed"),
      (
        Seed::from_persistence("pt").is_some(),
        "a bare PassThrough key parses as the empty seed",
      ),
    ])
  }
}
