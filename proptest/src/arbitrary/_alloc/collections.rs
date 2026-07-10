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

use crate::std_facade::{
    Arc, BTreeMap, BTreeSet, BinaryHeap, Box, LinkedList, Rc, Vec, VecDeque,
    binary_heap, btree_map, btree_set, fmt, linked_list, vec, vec_deque,
};
use core::ops::{Bound, RangeInclusive};

#[cfg(feature = "std")]
use crate::std_facade::{HashMap, HashSet, hash_map, hash_set};
#[cfg(feature = "std")]
use core::hash::Hash;

use crate::arbitrary::*;
use crate::collection::*;
use crate::strategy::statics::static_map;
use crate::strategy::*;

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
arbitrary!([A: Arbitrary + Hash + Eq, B: Arbitrary] HashMap<A, B>,
HashMapStrategy<A::Strategy, B::Strategy>,
RangedParams2<A::Parameters, B::Parameters>;
args => {
    let product_unpack![range, key_params, elem_params] = args;
    hash_map(any_with::<A>(key_params), any_with::<B>(elem_params), range)
});

#[cfg(feature = "std")]
arbitrary!([A: Arbitrary + Hash + Eq, B: Arbitrary] hash_map::IntoIter<A, B>,
    SMapped<HashMap<A, B>, Self>,
    <HashMap<A, B> as Arbitrary>::Parameters;
    args => static_map(any_with::<HashMap<A, B>>(args), HashMap::into_iter));

#[cfg(feature = "std")]
lift1!([, K: Hash + Eq + Arbitrary + 'static] HashMap<K, A>,
    RangedParams1<K::Parameters>;
    base, args => {
        let product_unpack![range, key_params] = args;
        hash_map(any_with::<K>(key_params), base, range)
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
impl<A: fmt::Debug + Eq + Hash, B: fmt::Debug> functor::ArbitraryF2<A, B>
    for HashMap<A, B>
{
    type Parameters = SizeRange;

    fn lift2_with<AS, BS>(
        fst: AS,
        snd: BS,
        args: Self::Parameters,
    ) -> BoxedStrategy<Self>
    where
        AS: Strategy<Value = A> + 'static,
        BS: Strategy<Value = B> + 'static,
    {
        hash_map(fst, snd, args).boxed()
    }
}

#[cfg(feature = "std")]
impl<A: fmt::Debug + Eq + Hash + 'static, B: fmt::Debug + 'static>
    functor::ArbitraryF2<A, B> for hash_map::IntoIter<A, B>
{
    type Parameters = SizeRange;

    fn lift2_with<AS, BS>(
        fst: AS,
        snd: BS,
        args: Self::Parameters,
    ) -> BoxedStrategy<Self>
    where
        AS: Strategy<Value = A> + 'static,
        BS: Strategy<Value = B> + 'static,
    {
        static_map(hash_map(fst, snd, args), HashMap::into_iter).boxed()
    }
}

//==============================================================================
// BTreeMap:
//==============================================================================

arbitrary!([A: Arbitrary + Ord, B: Arbitrary] BTreeMap<A, B>,
BTreeMapStrategy<A::Strategy, B::Strategy>,
RangedParams2<A::Parameters, B::Parameters>;
args => {
    let product_unpack![range, key_params, elem_params] = args;
    btree_map(any_with::<A>(key_params), any_with::<B>(elem_params), range)
});

lift1!([, K: Ord + Arbitrary + 'static] BTreeMap<K, A>,
    RangedParams1<K::Parameters>;
    base, args => {
        let product_unpack![range, key_params] = args;
        btree_map(any_with::<K>(key_params), base, range)
    }
);

impl<A: fmt::Debug + Ord, B: fmt::Debug> functor::ArbitraryF2<A, B>
    for BTreeMap<A, B>
{
    type Parameters = SizeRange;
    fn lift2_with<AS, BS>(
        fst: AS,
        snd: BS,
        args: Self::Parameters,
    ) -> BoxedStrategy<Self>
    where
        AS: Strategy<Value = A> + 'static,
        BS: Strategy<Value = B> + 'static,
    {
        btree_map(fst, snd, args).boxed()
    }
}

arbitrary!([A: Arbitrary + Ord, B: Arbitrary] btree_map::IntoIter<A, B>,
    SMapped<BTreeMap<A, B>, Self>,
    <BTreeMap<A, B> as Arbitrary>::Parameters;
    args => static_map(any_with::<BTreeMap<A, B>>(args), BTreeMap::into_iter));

impl<A: fmt::Debug + Ord + 'static, B: fmt::Debug + 'static>
    functor::ArbitraryF2<A, B> for btree_map::IntoIter<A, B>
{
    type Parameters = SizeRange;

    fn lift2_with<AS, BS>(
        fst: AS,
        snd: BS,
        args: Self::Parameters,
    ) -> BoxedStrategy<Self>
    where
        AS: Strategy<Value = A> + 'static,
        BS: Strategy<Value = B> + 'static,
    {
        static_map(btree_map(fst, snd, args), BTreeMap::into_iter).boxed()
    }
}

//==============================================================================
// Bound:
//==============================================================================

arbitrary!([A: Arbitrary] Bound<A>,
    TupleUnion<(
        WA<SFnPtrMap<Arc<A::Strategy>, Self>>,
        WA<SFnPtrMap<Arc<A::Strategy>, Self>>,
        WA<LazyJustFn<Self>>
    )>,
    A::Parameters;
    args => {
        let base = Arc::new(any_with::<A>(args));
        prop_oneof![
            2 => static_map(base.clone(), Bound::Included),
            2 => static_map(base, Bound::Excluded),
            1 => LazyJust::new(|| Bound::Unbounded),
        ]
    }
);

lift1!(['static] Bound<A>; base => {
    let base = Rc::new(base);
    prop_oneof![
        2 => base.clone().prop_map(Bound::Included),
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
