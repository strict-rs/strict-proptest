//-
// Copyright 2017, 2018, 2019, 2020, 2026 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

use crate::std_facade::{Arc, String, ToOwned, Vec, format, vec};
use crate::test_runner::config;
use core::convert::{Infallible, TryInto};
use core::result::Result;
use core::{fmt, str};
#[cfg(feature = "std")]
use rand::rand_core::UnwrapErr;
#[cfg(feature = "std")]
use rand::rngs::SysRng;
use rand::{Rng, RngExt, SeedableRng, TryRng};
use rand_chacha::ChaChaRng;
use rand_xorshift::XorShiftRng;

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
    /// It is faster than ChaCha but produces lower quality randomness and has
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
    pub(crate) fn persistence_key(self) -> &'static str {
        match self {
            RngAlgorithm::XorShift => "xs",
            RngAlgorithm::ChaCha => "cc",
            RngAlgorithm::PassThrough => "pt",
            RngAlgorithm::Recorder => "rc",
        }
    }

    /// The inverse of `persistence_key`: map a wire key back to its
    /// algorithm, or `None` if the key is unrecognized.
    pub(crate) fn from_persistence_key(key: &str) -> Option<Self> {
        match key {
            "xs" => Some(RngAlgorithm::XorShift),
            "cc" => Some(RngAlgorithm::ChaCha),
            "pt" => Some(RngAlgorithm::PassThrough),
            "rc" => Some(RngAlgorithm::Recorder),
            _ => None,
        }
    }
}

// These two are only used for parsing the environment variable
// PROPTEST_RNG_ALGORITHM.
impl str::FromStr for RngAlgorithm {
    type Err = ();
    fn from_str(s: &str) -> Result<Self, ()> {
        RngAlgorithm::from_persistence_key(s).ok_or(())
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
        off: usize,
        /// One past the last byte this generator may read.
        end: usize,
        /// The shared byte buffer, split across derived generators.
        data: Arc<[u8]>,
    },
    /// A `ChaCha` generator that also records the bytes it emits.
    Recorder {
        /// The underlying `ChaCha` generator.
        rng: ChaChaRng,
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
fn seeded_or_sys_rng<R: SeedableRng>(seed: config::RngSeed) -> R {
    match seed {
        config::RngSeed::Random => from_sys_rng::<R>(),
        config::RngSeed::Fixed(seed) => R::seed_from_u64(seed),
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
fn fill_entropy_seed<const N: usize, E>(
    mut seed: [u8; N],
    fill: impl FnOnce(&mut [u8]) -> Result<(), E>,
) -> ([u8; N], EntropySeedStatus) {
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

impl TestRng {
    /// Draw the next `u32`, dispatching to the active generator (and
    /// recording it under `Recorder`).
    fn next_u32_inner(&mut self) -> u32 {
        match &mut self.rng {
            TestRngImpl::XorShift(rng) => rng.next_u32(),
            TestRngImpl::ChaCha(rng) => rng.next_u32(),
            TestRngImpl::PassThrough { .. } => {
                let mut buf = [0; 4];
                self.fill_bytes_inner(&mut buf[..]);
                u32::from_le_bytes(buf)
            }
            TestRngImpl::Recorder { rng, record } => {
                let read = rng.next_u32();
                record.extend_from_slice(&read.to_le_bytes());
                read
            }
        }
    }

    /// Draw the next `u64`, dispatching to the active generator (and
    /// recording it under `Recorder`).
    fn next_u64_inner(&mut self) -> u64 {
        match &mut self.rng {
            TestRngImpl::XorShift(rng) => rng.next_u64(),
            TestRngImpl::ChaCha(rng) => rng.next_u64(),
            TestRngImpl::PassThrough { .. } => {
                let mut buf = [0; 8];
                self.fill_bytes_inner(&mut buf[..]);
                u64::from_le_bytes(buf)
            }
            TestRngImpl::Recorder { rng, record } => {
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
        match &mut self.rng {
            TestRngImpl::XorShift(rng) => rng.fill_bytes(dest),
            TestRngImpl::ChaCha(rng) => rng.fill_bytes(dest),
            TestRngImpl::PassThrough {
                off,
                end,
                data: bytes,
            } => {
                // Copy as much of the remaining window as fits in `dest`;
                // everything past the window (including the whole of `dest`
                // if the window is exhausted or inconsistent) reads as zero,
                // which is PassThrough's documented depletion behavior.
                let available = bytes.get(*off..*end).unwrap_or(&[]);
                let mut copied = 0;
                for (dst_byte, src_byte) in dest.iter_mut().zip(available) {
                    *dst_byte = *src_byte;
                    copied += 1;
                }
                *off += copied;
                for byte in dest.iter_mut().skip(copied) {
                    *byte = 0;
                }
            }
            TestRngImpl::Recorder { rng, record } => {
                rng.fill_bytes(dest);
                record.extend_from_slice(dest);
            }
        }
    }
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
pub(crate) enum Seed {
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
pub(crate) struct SeedLengthError {
    /// The algorithm name for the message (e.g. `"XorShift"`).
    algorithm: &'static str,
    /// The exact seed length that algorithm requires, in bytes.
    required: usize,
}

impl fmt::Display for SeedLengthError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} requires a {}-byte seed",
            self.algorithm, self.required
        )
    }
}

impl core::error::Error for SeedLengthError {}

impl Seed {
    /// Build a `Seed` for `algorithm` from `seed`, panicking on a
    /// wrong-length seed to preserve `TestRng::from_seed`'s documented
    /// contract; `try_from_bytes` is the non-panicking form.
    #[allow(
        clippy::single_call_fn,
        reason = "build a Seed from raw bytes, panicking to preserve TestRng::from_seed's documented contract"
    )]
    pub(crate) fn from_bytes(algorithm: RngAlgorithm, seed: &[u8]) -> Self {
        match Self::try_from_bytes(algorithm, seed) {
            Ok(parsed) => parsed,
            // This panic is `TestRng::from_seed`'s documented public
            // contract for a wrong-length seed.
            Err(error) => panic!("{}", error),
        }
    }

    /// Build a `Seed` for `algorithm` from `seed`, returning a
    /// `SeedLengthError` when the fixed-length algorithms are handed the
    /// wrong number of bytes (`PassThrough` accepts any length).
    #[allow(
        clippy::single_call_fn,
        reason = "parse raw bytes into a Seed, returning a typed length error instead of panicking"
    )]
    pub(crate) fn try_from_bytes(
        algorithm: RngAlgorithm,
        seed: &[u8],
    ) -> Result<Self, SeedLengthError> {
        match algorithm {
            RngAlgorithm::XorShift => seed
                .try_into()
                .map(Seed::XorShift)
                .map_err(|_| SeedLengthError {
                    algorithm: "XorShift",
                    required: 16,
                }),

            RngAlgorithm::ChaCha => {
                seed.try_into()
                    .map(Seed::ChaCha)
                    .map_err(|_| SeedLengthError {
                        algorithm: "ChaCha",
                        required: 32,
                    })
            }

            RngAlgorithm::PassThrough => {
                Ok(Seed::PassThrough(None, seed.into()))
            }

            RngAlgorithm::Recorder => seed
                .try_into()
                .map(Seed::Recorder)
                .map_err(|_| SeedLengthError {
                    algorithm: "Recorder",
                    required: 32,
                }),
        }
    }

    /// Decode a `Seed` from one persistence/replay line, or `None` if
    /// the algorithm key is unknown or its payload is malformed.
    pub(crate) fn from_persistence(string: &str) -> Option<Seed> {
        fn from_base16(dst: &mut [u8], src: &str) -> Option<()> {
            if dst.len() * 2 != src.len() {
                return None;
            }

            let (src_pairs, _) = src.as_bytes().as_chunks::<2>();
            for (dst_byte, src_pair) in dst.iter_mut().zip(src_pairs) {
                *dst_byte =
                    u8::from_str_radix(str::from_utf8(src_pair).ok()?, 16)
                        .ok()?;
            }

            Some(())
        }

        let parts =
            string.trim().split(char::is_whitespace).collect::<Vec<_>>();
        let (key, fields) = parts.split_first()?;
        RngAlgorithm::from_persistence_key(key).and_then(|alg| match alg {
            RngAlgorithm::XorShift => {
                if 4 != fields.len() {
                    return None;
                }

                let mut dwords = [0u32; 4];
                for (dword, part) in dwords.iter_mut().zip(fields) {
                    *dword = part.parse().ok()?;
                }

                let mut seed = [0u8; 16];
                let (seed_chunks, _) = seed.as_chunks_mut::<4>();
                for (chunk, dword) in seed_chunks.iter_mut().zip(dwords) {
                    *chunk = dword.to_le_bytes();
                }
                Some(Seed::XorShift(seed))
            }

            RngAlgorithm::ChaCha => {
                let [payload] = fields else { return None };
                let mut seed = [0u8; 32];
                from_base16(&mut seed, payload)?;
                Some(Seed::ChaCha(seed))
            }

            RngAlgorithm::PassThrough => match fields {
                [] => Some(Seed::PassThrough(None, vec![].into())),
                [payload] => {
                    let mut seed = vec![0u8; payload.len() / 2];
                    from_base16(&mut seed, payload)?;
                    Some(Seed::PassThrough(None, seed.into()))
                }
                _ => None,
            },

            RngAlgorithm::Recorder => {
                let [payload] = fields else { return None };
                let mut seed = [0u8; 32];
                from_base16(&mut seed, payload)?;
                Some(Seed::Recorder(seed))
            }
        })
    }

    /// Encode this `Seed` as its single-line persistence/replay form
    /// (the inverse of `from_persistence`).
    pub(crate) fn to_persistence(&self) -> String {
        fn to_base16(dst: &mut String, src: &[u8]) {
            for byte in src {
                dst.push_str(&format!("{:02x}", byte));
            }
        }

        match *self {
            Seed::XorShift(ref seed) => {
                let mut dwords = [0u32; 4];
                let (seed_chunks, _) = seed.as_chunks::<4>();
                for (dword, chunk) in dwords.iter_mut().zip(seed_chunks) {
                    *dword = u32::from_le_bytes(*chunk);
                }
                let [d0, d1, d2, d3] = dwords;
                format!(
                    "{} {} {} {} {}",
                    RngAlgorithm::XorShift.persistence_key(),
                    d0,
                    d1,
                    d2,
                    d3
                )
            }

            Seed::ChaCha(ref seed) => {
                let mut string =
                    RngAlgorithm::ChaCha.persistence_key().to_owned();
                string.push(' ');
                to_base16(&mut string, seed);
                string
            }

            Seed::PassThrough(bounds, ref bytes) => {
                // An inconsistent window (impossible via the tracked
                // consumption bounds) serializes as the exhausted seed.
                let bytes = bounds.map_or(bytes.as_ref(), |(start, end)| {
                    bytes.get(start..end).unwrap_or(&[])
                });
                let mut string =
                    RngAlgorithm::PassThrough.persistence_key().to_owned();
                string.push(' ');
                to_base16(&mut string, bytes);
                string
            }

            Seed::Recorder(ref seed) => {
                let mut string =
                    RngAlgorithm::Recorder.persistence_key().to_owned();
                string.push(' ');
                to_base16(&mut string, seed);
                string
            }
        }
    }
}

impl TestRng {
    /// Create a new RNG with the given algorithm and seed.
    ///
    /// Any RNG created with the same algorithm-seed pair will produce the same
    /// sequence of values on all systems and all supporting versions of
    /// proptest.
    ///
    /// ## Panics
    ///
    /// Panics if `seed` is not an appropriate length for `algorithm`.
    pub fn from_seed(algorithm: RngAlgorithm, seed: &[u8]) -> Self {
        TestRng::from_seed_internal(Seed::from_bytes(algorithm, seed))
    }

    /// Dumps the bytes obtained from the RNG so far (only works if the RNG is
    /// set to `Recorder`).
    ///
    /// ## Panics
    ///
    /// Panics if this RNG does not capture generated data.
    pub fn bytes_used(&self) -> Vec<u8> {
        match self.rng {
            TestRngImpl::Recorder { ref record, .. } => record.clone(),
            _ => panic!("bytes_used() called on non-Recorder RNG"),
        }
    }

    /// Construct a default TestRng from entropy.
    #[allow(
        clippy::single_call_fn,
        reason = "the default TestRng resolved from a configured seed and RNG algorithm"
    )]
    pub(crate) fn default_rng(
        seed: config::RngSeed,
        algorithm: RngAlgorithm,
    ) -> Self {
        #[cfg(feature = "std")]
        {
            Self {
                rng: match algorithm {
                    RngAlgorithm::XorShift => {
                        TestRngImpl::XorShift(seeded_or_sys_rng(seed))
                    }
                    RngAlgorithm::ChaCha => {
                        TestRngImpl::ChaCha(seeded_or_sys_rng(seed))
                    }
                    RngAlgorithm::PassThrough => {
                        panic!("cannot create default instance of PassThrough")
                    }
                    RngAlgorithm::Recorder => TestRngImpl::Recorder {
                        rng: seeded_or_sys_rng(seed),
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
            let _ = seed;
            Self::hardware_rng(algorithm)
        }
        #[cfg(all(
            not(feature = "std"),
            not(all(
                any(target_arch = "x86", target_arch = "x86_64"),
                feature = "hardware-rng"
            ))
        ))]
        {
            let _ = seed;
            Self::deterministic_rng(algorithm)
        }
    }

    /// The fixed 16-byte seed behind the deterministic `XorShift` RNG.
    const SEED_FOR_XOR_SHIFT: [u8; 16] = [
        0xf4, 0x16, 0x16, 0x48, 0xc3, 0xac, 0x77, 0xac, 0x72, 0x20, 0x0b, 0xea,
        0x99, 0x67, 0x2d, 0x6d,
    ];

    /// The fixed 32-byte seed behind the deterministic `ChaCha` (and
    /// `Recorder`) RNG.
    const SEED_FOR_CHA_CHA: [u8; 32] = [
        0xf4, 0x16, 0x16, 0x48, 0xc3, 0xac, 0x77, 0xac, 0x72, 0x20, 0x0b, 0xea,
        0x99, 0x67, 0x2d, 0x6d, 0xca, 0x9f, 0x76, 0xaf, 0x1b, 0x09, 0x73, 0xa0,
        0x59, 0x22, 0x6d, 0xc5, 0x46, 0x39, 0x1c, 0x4a,
    ];

    /// Returns a `TestRng` with a seed generated from `getrandom::fill`.
    ///
    /// This is useful in `no_std` scenarios on x86/x86_64 where consumers can
    /// select an entropy backend. OS-less targets that require RDRAND should
    /// build with `--cfg getrandom_backend="rdrand"`.
    ///
    /// ## Panics
    ///
    /// Panics if `algorithm` is `RngAlgorithm::PassThrough`, which has no
    /// deterministic seed.
    #[cfg(all(
        not(feature = "std"),
        any(target_arch = "x86", target_arch = "x86_64"),
        feature = "hardware-rng"
    ))]
    pub fn hardware_rng(algorithm: RngAlgorithm) -> Self {
        Self::from_seed_internal(match algorithm {
            RngAlgorithm::XorShift => {
                Seed::XorShift(hardware_seed(TestRng::SEED_FOR_XOR_SHIFT))
            }
            RngAlgorithm::ChaCha => {
                Seed::ChaCha(hardware_seed(TestRng::SEED_FOR_CHA_CHA))
            }
            RngAlgorithm::PassThrough => {
                panic!("deterministic RNG not available for PassThrough")
            }
            RngAlgorithm::Recorder => {
                Seed::Recorder(hardware_seed(TestRng::SEED_FOR_CHA_CHA))
            }
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
    ///
    /// ## Panics
    ///
    /// Panics if `algorithm` is `RngAlgorithm::PassThrough`, which has no
    /// deterministic seed.
    #[allow(
        clippy::single_call_fn,
        reason = "a fixed-seed TestRng for reproducible strategy and distribution tests"
    )]
    pub fn deterministic_rng(algorithm: RngAlgorithm) -> Self {
        Self::from_seed_internal(match algorithm {
            RngAlgorithm::XorShift => {
                Seed::XorShift(TestRng::SEED_FOR_XOR_SHIFT)
            }
            RngAlgorithm::ChaCha => Seed::ChaCha(TestRng::SEED_FOR_CHA_CHA),
            RngAlgorithm::PassThrough => {
                panic!("deterministic RNG not available for PassThrough")
            }
            RngAlgorithm::Recorder => Seed::Recorder(TestRng::SEED_FOR_CHA_CHA),
        })
    }

    /// Construct a TestRng by the perturbed randomized seed
    /// from an existing TestRng.
    pub(crate) fn gen_rng(&mut self) -> Self {
        Self::from_seed_internal(self.new_rng_seed())
    }

    /// Overwrite the given TestRng with the provided seed.
    pub(crate) fn set_seed(&mut self, seed: Seed) {
        *self = Self::from_seed_internal(seed);
    }

    /// Generate a new randomized seed, set it to this TestRng,
    /// and return the seed.
    pub(crate) fn gen_get_seed(&mut self) -> Seed {
        let seed = self.new_rng_seed();
        self.set_seed(seed.clone());
        seed
    }

    /// Randomize a perturbed randomized seed from the given TestRng.
    pub(crate) fn new_rng_seed(&mut self) -> Seed {
        match self.rng {
            TestRngImpl::XorShift(ref mut rng) => {
                let mut seed = rng.random::<[u8; 16]>();

                // Directly using XorShiftRng::from_seed() at this point would
                // result in rng and the returned value being exactly the same.
                // Perturb the seed with some arbitrary values to prevent this.
                let (words, _) = seed.as_chunks_mut::<4>();
                for [b0, b1, b2, b3] in words {
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
                let len = *end - *off;
                let child_start = *off + len / 2;
                let child_end = *off + len;
                *end = child_start;
                Seed::PassThrough(
                    Some((child_start, child_end)),
                    Arc::clone(bytes),
                )
            }

            TestRngImpl::Recorder { ref mut rng, .. } => {
                Seed::Recorder(rng.random())
            }
        }
    }

    /// Construct a TestRng from a given seed.
    fn from_seed_internal(seed: Seed) -> Self {
        Self {
            rng: match seed {
                Seed::XorShift(seed) => {
                    TestRngImpl::XorShift(XorShiftRng::from_seed(seed))
                }

                Seed::ChaCha(seed) => {
                    TestRngImpl::ChaCha(ChaChaRng::from_seed(seed))
                }

                Seed::PassThrough(bounds, bytes) => {
                    let (start, end) = bounds.unwrap_or((0, bytes.len()));
                    TestRngImpl::PassThrough {
                        off: start,
                        end,
                        data: bytes,
                    }
                }

                Seed::Recorder(seed) => TestRngImpl::Recorder {
                    rng: ChaChaRng::from_seed(seed),
                    record: Vec::new(),
                },
            },
        }
    }
}

#[cfg(test)]
mod test {
    use crate::std_facade::{Vec, vec};
    use std::borrow::ToOwned;
    use std::string::ToString;

    use rand::{Rng, RngExt};

    use super::{RngAlgorithm, Seed, TestRng};
    use crate::arbitrary::any;
    use crate::strategy::*;
    use strict_test_support::{
        TestFailure, ensure, ensure_all, ensure_eq, ensure_some,
    };

    #[test]
    fn gen_parse_seeds() -> Result<(), TestFailure> {
        let seeds = prop_oneof![
            any::<[u8; 16]>().prop_map(Seed::XorShift),
            any::<[u8; 32]>().prop_map(Seed::ChaCha),
            any::<Vec<u8>>()
                .prop_map(|raw| Seed::PassThrough(None, raw.into())),
            any::<[u8; 32]>().prop_map(Seed::Recorder),
        ];
        crate::strict::ensure_property(
            &seeds,
            "every seed round-trips through the persistence codec",
            |seed| {
                let parsed = ensure_some(
                    Seed::from_persistence(&seed.to_persistence()),
                    "a persisted seed parses back",
                )?;
                ensure(seed == parsed, "the parsed seed equals the original")
            },
        )
    }

    #[test]
    fn entropy_seed_fill_replaces_fallback_on_success()
    -> Result<(), TestFailure> {
        let fallback = [1u8, 2, 3, 4];
        let (seed, status) = super::fill_entropy_seed(fallback, |dest| {
            dest.copy_from_slice(&[9, 8, 7, 6]);
            Ok::<(), ()>(())
        });

        ensure(
            status == super::EntropySeedStatus::Filled,
            "successful entropy fill reports Filled",
        )?;
        ensure(
            seed == [9, 8, 7, 6],
            "successful entropy fill replaces the fallback seed",
        )
    }

    #[test]
    fn entropy_seed_fill_keeps_fallback_on_error() -> Result<(), TestFailure> {
        let fallback = [1u8, 2, 3, 4];
        let (seed, status) =
            super::fill_entropy_seed(fallback, |_dest| Err::<(), ()>(()));

        ensure(
            status == super::EntropySeedStatus::Fallback,
            "failed entropy fill reports Fallback",
        )?;
        ensure(
            seed == [1, 2, 3, 4],
            "failed entropy fill preserves the deterministic fallback seed",
        )
    }

    #[test]
    fn rngs_dont_clone_self_on_genrng() -> Result<(), TestFailure> {
        let seeds = prop_oneof![
            any::<[u8; 16]>().prop_map(Seed::XorShift),
            any::<[u8; 32]>().prop_map(Seed::ChaCha),
            Just(()).prop_perturb(|_, mut rng| {
                let mut buf = vec![0u8; 2048];
                rng.fill_bytes(&mut buf);
                Seed::PassThrough(None, buf.into())
            }),
            any::<[u8; 32]>().prop_map(Seed::Recorder),
        ];
        crate::strict::ensure_property(
            &seeds,
            "derived rngs never repeat their parent's stream",
            |seed| {
                type Value = [u8; 32];
                let orig = TestRng::from_seed_internal(seed);

                {
                    let mut rng1 = orig.clone();
                    let mut rng2 = rng1.gen_rng();
                    ensure(
                        rng1.random::<Value>() != rng2.random::<Value>(),
                        "a child rng differs from its parent",
                    )?;
                }

                let mut rng1 = orig.clone();
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
                    (
                        parent != grandchild,
                        "grandchild differs from the parent",
                    ),
                    (child != sibling, "siblings differ from each other"),
                    (
                        child != grandchild,
                        "grandchild differs from its parent's sibling",
                    ),
                    (
                        sibling != grandchild,
                        "second sibling differs from the grandchild",
                    ),
                ])
            },
        )
    }

    #[test]
    fn passthrough_rng_behaves_properly() -> Result<(), TestFailure> {
        let mut rng = TestRng::from_seed(
            RngAlgorithm::PassThrough,
            &[
                0xDE, 0xC0, 0x12, 0x34, 0x56, 0x78, 0xFE, 0xCA, 0xEF, 0xBE,
                0xAD, 0xDE, 0x01, 0x02, 0x03,
            ],
        );

        ensure_eq(
            &0x3412C0DE_u32,
            &rng.next_u32(),
            "the first dword replays the buffer little-endian",
        )?;
        ensure_eq(
            &0xDEADBEEFCAFE7856_u64,
            &rng.next_u64(),
            "the next qword continues the buffer",
        )?;

        let mut buf = [0u8; 4];
        rng.fill_bytes(&mut buf[0..4]);
        ensure(
            [1, 2, 3, 0] == buf,
            "fill_bytes drains the tail and zero-pads",
        )?;
        rng.fill_bytes(&mut buf[0..4]);
        ensure([0, 0, 0, 0] == buf, "a depleted buffer yields zeros")
    }

    #[test]
    fn seeded_xorshift_output_is_stable() -> Result<(), TestFailure> {
        let seed = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a,
            0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
        ];
        let mut rng_u32 = TestRng::from_seed(RngAlgorithm::XorShift, &seed);
        let mut rng_u64 = TestRng::from_seed(RngAlgorithm::XorShift, &seed);
        let mut rng_fill = TestRng::from_seed(RngAlgorithm::XorShift, &seed);

        ensure(
            [471271404, 722341711, 1880555887, 252576780]
                == [
                    rng_u32.next_u32(),
                    rng_u32.next_u32(),
                    rng_u32.next_u32(),
                    rng_u32.next_u32(),
                ],
            "the seeded xorshift u32 stream is stable",
        )?;
        ensure(
            [
                3102434025752954860,
                1084809011709542767,
                17342619095589341798,
                5127465042768897837,
            ] == [
                rng_u64.next_u64(),
                rng_u64.next_u64(),
                rng_u64.next_u64(),
                rng_u64.next_u64(),
            ],
            "the seeded xorshift u64 stream is stable",
        )?;

        let mut fill = [0u8; 16];
        rng_fill.fill_bytes(&mut fill);
        ensure(
            [
                236, 7, 23, 28, 79, 15, 14, 43, 111, 1, 23, 112, 12, 4, 14, 15,
            ] == fill,
            "the seeded xorshift fill_bytes output is stable",
        )
    }

    #[test]
    fn seeded_chacha_output_is_stable() -> Result<(), TestFailure> {
        let seed = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a,
            0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15,
            0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
        ];
        let mut rng_u32 = TestRng::from_seed(RngAlgorithm::ChaCha, &seed);
        let mut rng_u64 = TestRng::from_seed(RngAlgorithm::ChaCha, &seed);
        let mut rng_fill = TestRng::from_seed(RngAlgorithm::ChaCha, &seed);

        ensure(
            [2100034873, 1780073945, 1996733837, 1229642936]
                == [
                    rng_u32.next_u32(),
                    rng_u32.next_u32(),
                    rng_u32.next_u32(),
                    rng_u32.next_u32(),
                ],
            "the seeded chacha u32 stream is stable",
        )?;
        ensure(
            [
                7645359380336737593,
                5281276197874154893,
                14729830432180286858,
                10530800043416210610,
            ] == [
                rng_u64.next_u64(),
                rng_u64.next_u64(),
                rng_u64.next_u64(),
                rng_u64.next_u64(),
            ],
            "the seeded chacha u64 stream is stable",
        )?;

        let mut fill = [0u8; 16];
        rng_fill.fill_bytes(&mut fill);
        ensure(
            [
                57, 253, 43, 125, 217, 197, 25, 106, 141, 189, 3, 119, 184,
                220, 74, 73,
            ] == fill,
            "the seeded chacha fill_bytes output is stable",
        )
    }

    #[test]
    fn derived_child_rng_output_is_stable() -> Result<(), TestFailure> {
        let seed = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a,
            0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15,
            0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
        ];
        let mut parent = TestRng::from_seed(RngAlgorithm::ChaCha, &seed);
        let mut child = parent.gen_rng();

        ensure(
            [357635273, 1295757006, 1334659017, 3423482104]
                == [
                    child.next_u32(),
                    child.next_u32(),
                    child.next_u32(),
                    child.next_u32(),
                ],
            "the derived child rng stream is stable",
        )
    }

    #[test]
    fn recorder_bytes_used_matches_emitted_bytes() -> Result<(), TestFailure> {
        let seed = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a,
            0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10, 0x11, 0x12, 0x13, 0x14, 0x15,
            0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
        ];
        let mut rng = TestRng::from_seed(RngAlgorithm::Recorder, &seed);
        let first = rng.next_u32();
        let second = rng.next_u64();
        let mut fill = [0u8; 16];
        rng.fill_bytes(&mut fill);

        let mut expected = Vec::new();
        expected.extend_from_slice(&first.to_le_bytes());
        expected.extend_from_slice(&second.to_le_bytes());
        expected.extend_from_slice(&fill);

        ensure(
            expected == rng.bytes_used(),
            "the recorder replays exactly the bytes it emitted",
        )
    }

    #[test]
    fn try_from_bytes_accepts_correct_seed_lengths() -> Result<(), TestFailure>
    {
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
        let xorshift =
            Seed::try_from_bytes(RngAlgorithm::XorShift, &[0; 15]).err();
        let chacha = Seed::try_from_bytes(RngAlgorithm::ChaCha, &[0; 31]).err();
        let recorder =
            Seed::try_from_bytes(RngAlgorithm::Recorder, &[0; 33]).err();
        let render = |error: super::SeedLengthError| error.to_string();
        ensure_eq(
            &"XorShift requires a 16-byte seed".to_owned(),
            &ensure_some(xorshift, "a 15-byte XorShift seed is rejected")
                .map(render)?,
            "the XorShift length error keeps the legacy panic text",
        )?;
        ensure_eq(
            &"ChaCha requires a 32-byte seed".to_owned(),
            &ensure_some(chacha, "a 31-byte ChaCha seed is rejected")
                .map(render)?,
            "the ChaCha length error keeps the legacy panic text",
        )?;
        ensure_eq(
            &"Recorder requires a 32-byte seed".to_owned(),
            &ensure_some(recorder, "a 33-byte Recorder seed is rejected")
                .map(render)?,
            "the Recorder length error keeps the legacy panic text",
        )
    }

    #[test]
    fn persistence_parser_rejects_malformed_payloads() -> Result<(), TestFailure>
    {
        ensure_all(&[
            (
                Seed::from_persistence("cc abc").is_none(),
                "an odd-length base16 payload is rejected",
            ),
            (
                Seed::from_persistence("cc").is_none(),
                "a missing ChaCha payload is rejected",
            ),
            (
                Seed::from_persistence("xs 1 2 3").is_none(),
                "a short XorShift dword list is rejected",
            ),
            (
                Seed::from_persistence("").is_none(),
                "an empty line parses to no seed",
            ),
            (
                Seed::from_persistence("pt").is_some(),
                "a bare PassThrough key parses as the empty seed",
            ),
        ])
    }
}
