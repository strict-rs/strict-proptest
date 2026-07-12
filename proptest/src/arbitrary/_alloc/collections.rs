//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Arbitrary implementations for `std::collections`.

//#![cfg_attr(clippy, allow(implicit_hasher))]

//==============================================================================
// Imports:
//==============================================================================

#[cfg(feature = "std")]
use core::hash::BuildHasher;
#[cfg(feature = "std")]
use core::hash::Hash;
use core::ops::Bound;
use core::ops::RangeInclusive;

use crate::arbitrary::Arbitrary;
use crate::arbitrary::SFnPtrMap;
use crate::arbitrary::SMapped;
use crate::arbitrary::StrategyFor;
use crate::arbitrary::any;
use crate::arbitrary::any_with;
use crate::arbitrary::functor;
use crate::collection::BTreeMapStrategy;
use crate::collection::BTreeSetStrategy;
use crate::collection::BinaryHeapStrategy;
use crate::collection::HashMapStrategy;
use crate::collection::HashSetStrategy;
use crate::collection::LinkedListStrategy;
use crate::collection::SizeRange;
use crate::collection::VecDequeStrategy;
use crate::collection::VecStrategy;
use crate::collection::binary_heap;
use crate::collection::btree_map;
use crate::collection::btree_set;
use crate::collection::hash_map;
use crate::collection::hash_set;
use crate::collection::linked_list;
use crate::collection::vec;
use crate::collection::vec_deque;
use crate::std_facade::Arc;
use crate::std_facade::BTreeMap;
use crate::std_facade::BTreeSet;
use crate::std_facade::BinaryHeap;
use crate::std_facade::Box;
#[cfg(feature = "std")]
use crate::std_facade::HashMap;
#[cfg(feature = "std")]
use crate::std_facade::HashSet;
use crate::std_facade::LinkedList;
use crate::std_facade::Rc;
use crate::std_facade::Vec;
use crate::std_facade::VecDeque;
use crate::std_facade::binary_heap;
use crate::std_facade::btree_map;
use crate::std_facade::btree_set;
use crate::std_facade::fmt;
#[cfg(feature = "std")]
use crate::std_facade::hash_map;
#[cfg(feature = "std")]
use crate::std_facade::hash_set;
use crate::std_facade::linked_list;
use crate::std_facade::vec;
use crate::std_facade::vec_deque;
use crate::strategy::BoxedStrategy;
use crate::strategy::LazyJust;
use crate::strategy::LazyJustFn;
use crate::strategy::MapInto;
use crate::strategy::Strategy;
use crate::strategy::TupleUnion;
use crate::strategy::WeightedStrategy;
use crate::strategy::statics::static_map;

//==============================================================================
// Macros:
//==============================================================================

/// Parameters for configuring the generation of `StrategyFor<...<A>>`.
type RangedParams1<A> = product_type![SizeRange, A];

/// Parameters for configuring the generation of `StrategyFor<...<A, B>>`.
type RangedParams2<A, B> = product_type![SizeRange, A, B];

/// Implements `Arbitrary` (and the matching `lift1!`) for a single-element
/// collection type.
///
/// Given the collection (`Vec`, `VecDeque`, `BTreeSet`, ...), its `*Strategy`
/// type, any extra element bounds, and the `crate::collection` constructor, it
/// wires up `Parameters = RangedParams1<A>` (a `SizeRange` plus the element's
/// own params) and calls the constructor with an arbitrary element strategy
/// and that size range.
macro_rules! impl_1 {
    ($typ: ident, $strat: ident, $($bound : path),* => $fun: ident) => {
        arbitrary!([A: Arbitrary $(+ $bound)*] $typ<A>,
            $strat<A::Strategy>, RangedParams1<A::Parameters>;
            args => {
                let product_unpack![range, elem_params] = args;
                $fun(any_with::<A>(elem_params), range)
            });

        lift1!([$($bound+)*] $typ<A>, SizeRange;
            base, args => $fun(base, args));
    };
}

arbitrary!(SizeRange, MapInto<StrategyFor<RangeInclusive<usize>>, Self>;
    any::<RangeInclusive<usize>>().prop_map_into()
);

//==============================================================================
// Vec, VecDeque, LinkedList, BTreeSet, BinaryHeap, HashSet, HashMap:
//==============================================================================

/// Implements `Arbitrary` for the boxed-slice wrappers `Box<[A]>`, `Rc<[A]>`,
/// and `Arc<[A]>`.
///
/// Each reuses `Vec<A>`'s strategy and parameters, mapping the generated
/// vector into the slice wrapper with `prop_map_into` (`Strategy =
/// MapInto<StrategyFor<Vec<A>>, Self>`).
macro_rules! dst_wrapped {
    ($($w: ident),*) => {
        $(arbitrary!([A: Arbitrary] $w<[A]>,
            MapInto<StrategyFor<Vec<A>>, Self>,
            <Vec<A> as Arbitrary>::Parameters;
            args => any_with::<Vec<A>>(args).prop_map_into()
        );)*
    };
}

impl_1!(Vec, VecStrategy, => vec);
dst_wrapped!(Box, Rc, Arc);
impl_1!(VecDeque, VecDequeStrategy, => vec_deque);
impl_1!(LinkedList, LinkedListStrategy, => linked_list);
impl_1!(BTreeSet, BTreeSetStrategy, Ord => btree_set);
impl_1!(BinaryHeap, BinaryHeapStrategy, Ord => binary_heap);
#[cfg(feature = "std")]
impl_1!(HashSet, HashSetStrategy, Hash, Eq => hash_set);

//==============================================================================
// IntoIterator:
//==============================================================================

/// Implements `Arbitrary` (and the matching `lift1!`) for a collection's
/// owning `IntoIter`.
///
/// Generates an arbitrary collection of the given type and maps it through
/// `into_iter` (`Strategy = SMapped<$type<A>, Self>`), reusing the
/// collection's own parameters.
macro_rules! into_iter_1 {
    ($module: ident, $type: ident $(, $bound : path)*) => {
        arbitrary!([A: Arbitrary $(+ $bound)*]
            $module::IntoIter<A>,
            SMapped<$type<A>, Self>,
            <$type<A> as Arbitrary>::Parameters;
            args => static_map(any_with::<$type<A>>(args), $type::into_iter));

        lift1!(['static + $($bound+)*] $module::IntoIter<A>, SizeRange;
            base, args =>
                $module(base, args).prop_map($type::into_iter));
    };
}

into_iter_1!(vec, Vec);
into_iter_1!(vec_deque, VecDeque);
into_iter_1!(linked_list, LinkedList);
into_iter_1!(btree_set, BTreeSet, Ord);
into_iter_1!(binary_heap, BinaryHeap, Ord);
#[cfg(feature = "std")]
into_iter_1!(hash_set, HashSet, Hash, Eq);

//==============================================================================
// HashMap:
//==============================================================================

#[cfg(feature = "std")]
/// Rebuild a generated default-hasher map with the caller's hasher type.
fn hash_map_with_hasher<K: Hash + Eq, V, S: BuildHasher + Default>(map: HashMap<K, V>) -> HashMap<K, V, S> {
  map.into_iter().collect()
}

#[cfg(feature = "std")]
arbitrary!([K: Arbitrary + Hash + Eq, V: Arbitrary, S: BuildHasher + Default] HashMap<K, V, S>,
SFnPtrMap<HashMapStrategy<K::Strategy, V::Strategy>, Self>,
RangedParams2<K::Parameters, V::Parameters>;
args => {
    let product_unpack![range, key_params, elem_params] = args;
    static_map(
        hash_map(any_with::<K>(key_params), any_with::<V>(elem_params), range),
        hash_map_with_hasher::<K, V, S>,
    )
});

#[cfg(feature = "std")]
arbitrary!([K: Arbitrary + Hash + Eq, V: Arbitrary] hash_map::IntoIter<K, V>,
    SMapped<HashMap<K, V>, Self>,
    <HashMap<K, V> as Arbitrary>::Parameters;
    args => static_map(any_with::<HashMap<K, V>>(args), HashMap::into_iter));

#[cfg(feature = "std")]
lift1!(['static, K: Hash + Eq + Arbitrary + 'static, H: BuildHasher + Default + 'static] HashMap<K, A, H>,
    RangedParams1<K::Parameters>;
    base, args => {
        let product_unpack![range, key_params] = args;
        static_map(
            hash_map(any_with::<K>(key_params), base, range),
            hash_map_with_hasher::<K, A, H>,
        )
    }
);

#[cfg(feature = "std")]
lift1!(['static, K: Hash + Eq + Arbitrary + 'static] hash_map::IntoIter<K, A>,
    RangedParams1<K::Parameters>;
    base, args => {
        let product_unpack![range, key_params] = args;
        static_map(hash_map(any_with::<K>(key_params), base, range), HashMap::into_iter)
    }
);

#[cfg(feature = "std")]
impl<K: fmt::Debug + Eq + Hash + 'static, V: fmt::Debug + 'static, S: BuildHasher + Default + 'static> functor::ArbitraryF2<K, V>
  for HashMap<K, V, S>
{
  type Parameters = SizeRange;

  fn lift2_with<AS, BS>(fst: AS, snd: BS, args: Self::Parameters) -> BoxedStrategy<Self>
  where
    AS: Strategy<Value = K> + 'static,
    BS: Strategy<Value = V> + 'static,
  {
    static_map(hash_map(fst, snd, args), hash_map_with_hasher::<K, V, S>).boxed()
  }
}

#[cfg(feature = "std")]
impl<K: fmt::Debug + Eq + Hash + 'static, V: fmt::Debug + 'static> functor::ArbitraryF2<K, V> for hash_map::IntoIter<K, V> {
  type Parameters = SizeRange;

  fn lift2_with<AS, BS>(fst: AS, snd: BS, args: Self::Parameters) -> BoxedStrategy<Self>
  where
    AS: Strategy<Value = K> + 'static,
    BS: Strategy<Value = V> + 'static,
  {
    static_map(hash_map(fst, snd, args), HashMap::into_iter).boxed()
  }
}

//==============================================================================
// BTreeMap:
//==============================================================================

arbitrary!([K: Arbitrary + Ord, V: Arbitrary] BTreeMap<K, V>,
BTreeMapStrategy<K::Strategy, V::Strategy>,
RangedParams2<K::Parameters, V::Parameters>;
args => {
    let product_unpack![range, key_params, elem_params] = args;
    btree_map(any_with::<K>(key_params), any_with::<V>(elem_params), range)
});

lift1!([, K: Ord + Arbitrary + 'static] BTreeMap<K, A>,
    RangedParams1<K::Parameters>;
    base, args => {
        let product_unpack![range, key_params] = args;
        btree_map(any_with::<K>(key_params), base, range)
    }
);

impl<K: fmt::Debug + Ord, V: fmt::Debug> functor::ArbitraryF2<K, V> for BTreeMap<K, V> {
  type Parameters = SizeRange;
  fn lift2_with<AS, BS>(fst: AS, snd: BS, args: Self::Parameters) -> BoxedStrategy<Self>
  where
    AS: Strategy<Value = K> + 'static,
    BS: Strategy<Value = V> + 'static,
  {
    btree_map(fst, snd, args).boxed()
  }
}

arbitrary!([K: Arbitrary + Ord, V: Arbitrary] btree_map::IntoIter<K, V>,
    SMapped<BTreeMap<K, V>, Self>,
    <BTreeMap<K, V> as Arbitrary>::Parameters;
    args => static_map(any_with::<BTreeMap<K, V>>(args), BTreeMap::into_iter));

impl<K: fmt::Debug + Ord + 'static, V: fmt::Debug + 'static> functor::ArbitraryF2<K, V> for btree_map::IntoIter<K, V> {
  type Parameters = SizeRange;

  fn lift2_with<AS, BS>(fst: AS, snd: BS, args: Self::Parameters) -> BoxedStrategy<Self>
  where
    AS: Strategy<Value = K> + 'static,
    BS: Strategy<Value = V> + 'static,
  {
    static_map(btree_map(fst, snd, args), BTreeMap::into_iter).boxed()
  }
}

//==============================================================================
// Bound:
//==============================================================================

arbitrary!([A: Arbitrary] Bound<A>,
    TupleUnion<(
        WeightedStrategy<SFnPtrMap<Rc<A::Strategy>, Self>>,
        WeightedStrategy<SFnPtrMap<Rc<A::Strategy>, Self>>,
        WeightedStrategy<LazyJustFn<Self>>
    )>,
    A::Parameters;
    args => {
        let base = Rc::new(any_with::<A>(args));
        prop_oneof![
            2 => static_map(Rc::clone(&base), Bound::Included),
            2 => static_map(base, Bound::Excluded),
            1 => LazyJust::new(|| Bound::Unbounded),
        ]
    }
);

lift1!(['static] Bound<A>; base => {
    let base = Rc::new(base);
    prop_oneof![
        2 => Rc::clone(&base).prop_map(Bound::Included),
        2 => base.prop_map(Bound::Excluded),
        1 => LazyJustFn::new(|| Bound::Unbounded),
    ]
});

#[cfg(test)]
mod test {
  use super::*;

  no_panic_test!(
      size_bounds => SizeRange,
      vec => Vec<u8>,
      box_slice => Box<[u8]>,
      rc_slice  => Rc<[u8]>,
      arc_slice  => Arc<[u8]>,
      vec_deque => VecDeque<u8>,
      linked_list => LinkedList<u8>,
      btree_set => BTreeSet<u8>,
      btree_map => BTreeMap<u8, u8>,
      bound => Bound<u8>,
      binary_heap => BinaryHeap<u8>,
      into_iter_vec => vec::IntoIter<u8>,
      into_iter_vec_deque => vec_deque::IntoIter<u8>,
      into_iter_linked_list => linked_list::IntoIter<u8>,
      into_iter_binary_heap => binary_heap::IntoIter<u8>,
      into_iter_btree_set => btree_set::IntoIter<u8>,
      into_iter_btree_map => btree_map::IntoIter<u8, u8>
  );

  #[cfg(feature = "std")]
  no_panic_test!(
      hash_set => HashSet<u8>,
      hash_map => HashMap<u8, u8>,
      into_iter_hash_set => hash_set::IntoIter<u8>,
      into_iter_hash_map => hash_map::IntoIter<u8, u8>
  );
}
