//-
// Copyright 2026 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Stable substitute types for APIs that are still nightly-only in `std`.
//!
//! These types intentionally do not claim type identity with the nightly
//! standard-library APIs. They provide stable shapes for generation and tests
//! when `alt-stable` is enabled.

#[cfg(feature = "std")]
use std::net::Ipv6Addr;

/// Stable substitute for `core::ops::CoroutineState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CoroutineState<Y, R> {
  /// The coroutine yielded `Y`.
  Yielded(Y),
  /// The coroutine completed with `R`.
  Complete(R),
}

/// Stable substitute for `std::net::Ipv6MulticastScope`.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Ipv6MulticastScope {
  /// Interface-local multicast scope.
  InterfaceLocal,
  /// Link-local multicast scope.
  LinkLocal,
  /// Realm-local multicast scope.
  RealmLocal,
  /// Admin-local multicast scope.
  AdminLocal,
  /// Site-local multicast scope.
  SiteLocal,
  /// Organization-local multicast scope.
  OrganizationLocal,
  /// Global multicast scope.
  Global,
}

impl Ipv6MulticastScope {
  /// Classify an IPv6 multicast scope nibble.
  #[must_use]
  #[allow(
    clippy::single_call_fn,
    reason = "public scope-nibble classifier documents and exposes the stable substitute's core mapping"
  )]
  pub const fn from_scope_nibble(scope: u8) -> Option<Self> {
    match scope {
      1 => Some(Self::InterfaceLocal),
      2 => Some(Self::LinkLocal),
      3 => Some(Self::RealmLocal),
      4 => Some(Self::AdminLocal),
      5 => Some(Self::SiteLocal),
      8 => Some(Self::OrganizationLocal),
      0xE => Some(Self::Global),
      _ => None,
    }
  }

  /// Classify the multicast scope of an IPv6 address.
  #[cfg(feature = "std")]
  #[must_use]
  #[allow(
    clippy::single_call_fn,
    reason = "the address-level API checks the multicast prefix before decoding its scope nibble"
  )]
  pub const fn from_ipv6_addr(addr: Ipv6Addr) -> Option<Self> {
    let octets = addr.octets();
    if octets[0] != 0xff {
      return None;
    }
    Self::from_scope_nibble(octets[1] & 0x0f)
  }
}

#[cfg(test)]
mod test {
  #[cfg(feature = "std")]
  use std::net::Ipv6Addr;

  use strict_test_support::PredicateFailure;
  use strict_test_support::ensure_that;

  use crate::std_facade::Vec;

  /// Input nibble, observed classification, and expected classification.
  type ScopeObservation = (u8, Option<Ipv6MulticastScope>, Option<Ipv6MulticastScope>);

  use super::Ipv6MulticastScope;

  /// Native address classifications at the multicast-prefix boundary.
  #[cfg(feature = "std")]
  type AddressScopes = [(Ipv6Addr, Option<Ipv6MulticastScope>); 2];

  #[test]
  fn scope_nibble_accepts_only_named_scopes() -> Result<(), PredicateFailure<Vec<ScopeObservation>>> {
    let accepted = [
      (1, Ipv6MulticastScope::InterfaceLocal),
      (2, Ipv6MulticastScope::LinkLocal),
      (3, Ipv6MulticastScope::RealmLocal),
      (4, Ipv6MulticastScope::AdminLocal),
      (5, Ipv6MulticastScope::SiteLocal),
      (8, Ipv6MulticastScope::OrganizationLocal),
      (0xE, Ipv6MulticastScope::Global),
    ];

    let observations = accepted
      .into_iter()
      .map(|(scope, expected)| (scope, Some(expected)))
      .chain([0, 6, 7, 9, 0xA, 0xB, 0xC, 0xD, 0xF].into_iter().map(|scope| (scope, None)))
      .map(|(scope, expected)| (scope, Ipv6MulticastScope::from_scope_nibble(scope), expected))
      .collect();
    ensure_that(
      observations,
      "named scope nibbles classify and unnamed nibbles are rejected",
      |subjects: &Vec<ScopeObservation>| subjects.iter().all(|observed| observed.1 == observed.2),
    )
    .map(drop)
  }

  #[cfg(feature = "std")]
  #[test]
  fn ipv6_addr_scope_requires_multicast_prefix() -> Result<(), PredicateFailure<AddressScopes>> {
    let observations = [Ipv6Addr::new(0xff0e, 0, 0, 0, 0, 0, 0, 1), Ipv6Addr::LOCALHOST]
      .map(|address| (address, Ipv6MulticastScope::from_ipv6_addr(address)));
    ensure_that(
      observations,
      "multicast ff0e is global and localhost has no multicast scope",
      |subjects| matches!(*subjects, [(_, Some(Ipv6MulticastScope::Global)), (_, None)]),
    )
    .map(drop)
  }
}
