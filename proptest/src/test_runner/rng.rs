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

/// Generate word reads with the native algorithm's word method so seeded
/// streams keep their consumption order. Replay and recording use little-endian
/// bytes for the same word width.
macro_rules! rng_word {
  ($method:ident, $word:ty, $native:ident) => {
    fn $method(&mut self) -> Result<$word, Self::Error> {
      Ok(match self.rng {
        TestRngImpl::XorShift(ref mut rng) => rng.$native(),
        TestRngImpl::ChaCha(ref mut rng) => rng.$native(),
        TestRngImpl::PassThrough {
          ..
        } => {
          let mut bytes = [0; size_of::<$word>()];
          self.fill_bytes_inner(&mut bytes);
          <$word>::from_le_bytes(bytes)
        }
        TestRngImpl::Recorder {
          ref mut rng,
          ref mut record,
        } => {
          let value = rng.$native();
          record.extend_from_slice(&value.to_le_bytes());
          value
        }
      })
    }
  };
}

impl TryRng for TestRng {
  type Error = Infallible;

  rng_word!(try_next_u32, u32, next_u32);
  rng_word!(try_next_u64, u64, next_u64);

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
  use core::array::from_fn;
  use std::string::ToString as _;

  use rand::Rng as _;
  #[cfg(feature = "strict-test")]
  use rand::RngExt as _;
  use strict_test_support::ComparisonFailure;
  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_eq;
  use strict_test_support::ensure_that;

  use super::EntropySeedStatus;
  use super::RngAlgorithm;
  use super::Seed;
  use super::SeedLengthError;
  use super::TestRng;
  #[cfg(feature = "strict-test")]
  use crate::arbitrary::any;
  use crate::std_facade::Box;
  #[cfg(feature = "strict-test")]
  use crate::std_facade::String;
  use crate::std_facade::Vec;
  #[cfg(feature = "strict-test")]
  use crate::std_facade::vec;
  #[cfg(feature = "strict-test")]
  use crate::strategy::*;
  #[cfg(feature = "strict-test")]
  use crate::strict::ensure_property;
  #[cfg(feature = "strict-test")]
  use crate::test_runner::PropertyResult;

  /// Complete concrete assertion failures remain allocated.
  type Check<S> = Result<(), Box<PredicateFailure<S>>>;
  /// Native comparison operands stay together without a large stack error.
  type Equality<L, R = L> = Result<(), Box<ComparisonFailure<L, R>>>;
  /// Entropy filling retains both bytes and the fallback decision.
  type Entropy = ([u8; 4], EntropySeedStatus);
  /// The original seed, wire representation, and parsed seed.
  #[cfg(feature = "strict-test")]
  type SeedRoundTrip = (Seed, String, Option<Seed>);
  /// Seeded properties retain complete success and failure subjects.
  #[cfg(feature = "strict-test")]
  type SeedProperty<S> = PropertyResult<Seed, S, Box<PredicateFailure<S>>>;
  /// All parent, sibling, and grandchild rngs and their emitted bytes.
  #[cfg(feature = "strict-test")]
  type Streams = (Seed, [(TestRng, [u8; 32]); 6]);
  /// Recorder state and each emitted value alongside the captured bytes.
  type Recording = (TestRng, u32, u64, [u8; 16], Option<Vec<u8>>);
  /// Malformed seeds and the valid empty pass-through seed.
  type ParsedSeeds = [Option<Seed>; 5];

  #[cfg(feature = "strict-test")]
  #[test]
  fn gen_parse_seeds() -> SeedProperty<SeedRoundTrip> {
    let seeds = prop_oneof![
      any::<[u8; 16]>().prop_map(Seed::XorShift),
      any::<[u8; 32]>().prop_map(Seed::ChaCha),
      any::<Vec<u8>>().prop_map(|raw| Seed::PassThrough(None, raw.into())),
      any::<[u8; 32]>().prop_map(Seed::Recorder),
    ];
    ensure_property(&seeds, "every seed round-trips through the persistence codec", |seed| {
      let wire = seed.to_persistence();
      let parsed = Seed::from_persistence(&wire);
      ensure_that((seed, wire, parsed), "the parsed seed equals the original", |observed| {
        observed.2.as_ref() == Some(&observed.0)
      })
      .map_err(Box::new)
    })
  }

  #[test]
  fn entropy_seed_fill_replaces_fallback_on_success() -> Equality<Entropy, Entropy> {
    let fallback = [1_u8, 2, 3, 4];
    let (seed, status) = super::fill_entropy_seed(fallback, |dest| {
      for (slot, value) in dest.iter_mut().zip([9_u8, 8, 7, 6]) {
        *slot = value;
      }
      Ok::<(), ()>(())
    });

    ensure_eq(
      (seed, status),
      ([9, 8, 7, 6], EntropySeedStatus::Filled),
      "successful entropy fill replaces the fallback seed",
    )
    .map_err(Box::new)
    .map(drop)
  }

  #[test]
  fn entropy_seed_fill_keeps_fallback_on_error() -> Equality<Entropy, Entropy> {
    let fallback = [1_u8, 2, 3, 4];
    let (seed, status) = super::fill_entropy_seed(fallback, |_dest| Err::<(), ()>(()));

    ensure_eq(
      (seed, status),
      (fallback, EntropySeedStatus::Fallback),
      "failed entropy fill preserves the fallback seed",
    )
    .map_err(Box::new)
    .map(drop)
  }

  #[cfg(feature = "strict-test")]
  #[test]
  fn rngs_dont_clone_self_on_genrng() -> SeedProperty<Streams> {
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
      let orig = TestRng::from_seed_internal(seed.clone());
      let mut parent_clone = orig.clone();
      let first_child = parent_clone.gen_rng();
      let mut parent = orig;
      let mut child = parent.gen_rng();
      let sibling = parent.gen_rng();
      let grandchild = child.gen_rng();
      let streams = [parent_clone, first_child, parent, child, sibling, grandchild].map(|mut rng| {
        let bytes = rng.random::<[u8; 32]>();
        (rng, bytes)
      });
      ensure_that(
        (seed, streams),
        "derived streams differ from parents, siblings, and grandchildren",
        |observed| {
          let [
            original_bytes,
            first_bytes,
            parent_bytes,
            child_bytes,
            sibling_bytes,
            grandchild_bytes,
          ] = observed.1.each_ref().map(|stream| stream.1);
          original_bytes != first_bytes
            && parent_bytes != child_bytes
            && parent_bytes != sibling_bytes
            && parent_bytes != grandchild_bytes
            && child_bytes != sibling_bytes
            && child_bytes != grandchild_bytes
            && sibling_bytes != grandchild_bytes
        },
      )
      .map_err(Box::new)
    })
  }

  /// Mixed-width reads and the buffers observed before and after depletion.
  type PassThroughReads = (u32, u64, [u8; 4], [u8; 4]);

  #[test]
  fn passthrough_rng_behaves_properly() -> Equality<PassThroughReads, PassThroughReads> {
    let mut rng = TestRng::from_seed(RngAlgorithm::PassThrough, &[
      0xDE, 0xC0, 0x12, 0x34, 0x56, 0x78, 0xFE, 0xCA, 0xEF, 0xBE, 0xAD, 0xDE, 0x01, 0x02, 0x03,
    ]);
    let first = rng.next_u32();
    let second = rng.next_u64();
    let mut tail = [0; 4];
    rng.fill_bytes(&mut tail);
    let mut depleted = [0; 4];
    rng.fill_bytes(&mut depleted);
    ensure_eq(
      (first, second, tail, depleted),
      (0x3412_C0DE, 0xDEAD_BEEF_CAFE_7856, [1, 2, 3, 0], [0; 4]),
      "PassThrough replays little-endian values then zero-pads its depleted buffer",
    )
    .map_err(Box::new)
    .map(drop)
  }

  /// Both word widths retain their generators and reads across depletion.
  type DepletedWords = [(TestRng, [u32; 2], TestRng, [u64; 2]); 3];

  #[test]
  fn passthrough_words_zero_pad_missing_bytes() -> Check<DepletedWords> {
    let seeds: [&[u8]; 3] = [&[], &[0xa5], &[1, 2, 3]];
    let observations = seeds.map(|seed| {
      let mut dword_rng = TestRng::from_seed(RngAlgorithm::PassThrough, seed);
      let mut qword_rng = TestRng::from_seed(RngAlgorithm::PassThrough, seed);
      let dwords = from_fn(|_| dword_rng.next_u32());
      let qwords = from_fn(|_| qword_rng.next_u64());
      (dword_rng, dwords, qword_rng, qwords)
    });
    ensure_that(
      observations,
      "partial and empty replay words are little-endian with zero-filled missing bytes",
      |observed| {
        observed
          .iter()
          .zip([0, 0xa5, 0x0003_0201])
          .all(|(reads, expected)| reads.1 == [expected, 0] && reads.3 == [u64::from(expected), 0])
      },
    )
    .map_err(Box::new)
    .map(drop)
  }

  /// The three public reading forms of a fixed seeded stream.
  type StableReads = ([u32; 4], [u64; 4], [u8; 16]);

  /// Observe the same seed through each public reading form.
  fn stable_reads(algorithm: RngAlgorithm, seed: &[u8]) -> StableReads {
    let mut rng_u32 = TestRng::from_seed(algorithm, seed);
    let mut rng_u64 = TestRng::from_seed(algorithm, seed);
    let mut rng_fill = TestRng::from_seed(algorithm, seed);
    let dwords = from_fn(|_| rng_u32.next_u32());
    let qwords = from_fn(|_| rng_u64.next_u64());
    let mut fill = [0; 16];
    rng_fill.fill_bytes(&mut fill);
    (dwords, qwords, fill)
  }

  #[test]
  fn seeded_xorshift_output_is_stable() -> Equality<StableReads, StableReads> {
    let seed = [
      0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    ];
    ensure_eq(
      stable_reads(RngAlgorithm::XorShift, &seed),
      (
        [471_271_404, 722_341_711, 1_880_555_887, 252_576_780],
        [
          3_102_434_025_752_954_860, 1_084_809_011_709_542_767, 17_342_619_095_589_341_798, 5_127_465_042_768_897_837,
        ],
        [236, 7, 23, 28, 79, 15, 14, 43, 111, 1, 23, 112, 12, 4, 14, 15],
      ),
      "the seeded XorShift stream is stable across reading forms",
    )
    .map_err(Box::new)
    .map(drop)
  }

  #[test]
  fn seeded_chacha_output_is_stable() -> Equality<StableReads, StableReads> {
    let seed = [
      0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15,
      0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
    ];
    ensure_eq(
      stable_reads(RngAlgorithm::ChaCha, &seed),
      (
        [2_100_034_873, 1_780_073_945, 1_996_733_837, 1_229_642_936],
        [
          7_645_359_380_336_737_593, 5_281_276_197_874_154_893, 14_729_830_432_180_286_858, 10_530_800_043_416_210_610,
        ],
        [57, 253, 43, 125, 217, 197, 25, 106, 141, 189, 3, 119, 184, 220, 74, 73],
      ),
      "the seeded ChaCha stream is stable across reading forms",
    )
    .map_err(Box::new)
    .map(drop)
  }

  #[test]
  fn derived_child_rng_output_is_stable() -> Equality<[u32; 4], [u32; 4]> {
    let seed = [
      0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15,
      0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
    ];
    let mut parent = TestRng::from_seed(RngAlgorithm::ChaCha, &seed);
    let mut child = parent.gen_rng();

    ensure_eq(
      [child.next_u32(), child.next_u32(), child.next_u32(), child.next_u32()],
      [357_635_273, 1_295_757_006, 1_334_659_017, 3_423_482_104],
      "the derived child rng stream is stable",
    )
    .map_err(Box::new)
    .map(drop)
  }

  #[test]
  fn recorder_bytes_used_matches_emitted_bytes() -> Check<Recording> {
    let seed = [
      0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15,
      0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
    ];
    let mut rng = TestRng::from_seed(RngAlgorithm::Recorder, &seed);
    let first = rng.next_u32();
    let second = rng.next_u64();
    let mut fill = [0_u8; 16];
    rng.fill_bytes(&mut fill);

    let bytes_used = rng.bytes_used();
    ensure_that(
      (rng, first, second, fill, bytes_used),
      "the recorder retains exactly the bytes emitted by every reading form",
      |observed| {
        observed.4.as_ref().is_some_and(|bytes| {
          bytes.iter().copied().eq(
            observed
              .1
              .to_le_bytes()
              .into_iter()
              .chain(observed.2.to_le_bytes())
              .chain(observed.3),
          )
        })
      },
    )
    .map_err(Box::new)
    .map(drop)
  }

  /// Complete exact-length seed construction outcomes.
  type SeedResults<const N: usize> = [Result<Seed, SeedLengthError>; N];

  #[test]
  fn try_from_bytes_accepts_correct_seed_lengths() -> Equality<SeedResults<4>, SeedResults<4>> {
    ensure_eq(
      [
        Seed::try_from_bytes(RngAlgorithm::XorShift, &[0; 16]),
        Seed::try_from_bytes(RngAlgorithm::ChaCha, &[0; 32]),
        Seed::try_from_bytes(RngAlgorithm::Recorder, &[0; 32]),
        Seed::try_from_bytes(RngAlgorithm::PassThrough, &[]),
      ],
      [
        Ok(Seed::XorShift([0; 16])),
        Ok(Seed::ChaCha([0; 32])),
        Ok(Seed::Recorder([0; 32])),
        Ok(Seed::PassThrough(None, [].into())),
      ],
      "exact-length seeds preserve their algorithm and bytes; PassThrough also accepts empty seeds",
    )
    .map_err(Box::new)
    .map(drop)
  }

  #[test]
  fn try_from_bytes_rejects_wrong_seed_lengths() -> Check<SeedResults<3>> {
    let subject = [
      Seed::try_from_bytes(RngAlgorithm::XorShift, &[0; 15]),
      Seed::try_from_bytes(RngAlgorithm::ChaCha, &[0; 31]),
      Seed::try_from_bytes(RngAlgorithm::Recorder, &[0; 33]),
    ];
    let matches_length = |result: &Result<Seed, SeedLengthError>,
                          (algorithm, required, actual, message): (&'static str, usize, usize, &'static str)| {
      let Err(ref error) = *result else {
        return false;
      };
      *error
        == SeedLengthError {
          algorithm,
          required,
          actual,
        }
        && error.to_string() == message
    };
    ensure_that(
      subject,
      "length errors preserve and render the required and supplied sizes",
      |results| {
        results
          .iter()
          .zip([
            ("XorShift", 16, 15, "XorShift requires a 16-byte seed, got 15 bytes"),
            ("ChaCha", 32, 31, "ChaCha requires a 32-byte seed, got 31 bytes"),
            ("Recorder", 32, 33, "Recorder requires a 32-byte seed, got 33 bytes"),
          ])
          .all(|(result, expected)| matches_length(result, expected))
      },
    )
    .map_err(Box::new)
    .map(drop)
  }

  #[test]
  fn persistence_parser_rejects_malformed_payloads() -> Equality<ParsedSeeds, ParsedSeeds> {
    ensure_eq(
      ["cc abc", "cc", "xs 1 2 3", "", "pt"].map(Seed::from_persistence),
      [None, None, None, None, Some(Seed::PassThrough(None, [].into()))],
      "malformed payloads are rejected while a bare PassThrough key denotes an empty seed",
    )
    .map_err(Box::new)
    .map(drop)
  }
}
