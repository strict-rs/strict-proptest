//-
// Copyright 2017, 2018 The proptest developers
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Arbitrary implementations for `std::cell`.

use core::cell::{BorrowError, BorrowMutError, Cell, RefCell, UnsafeCell};

wrap_from!([Copy] Cell);
wrap_from!(RefCell);
wrap_from!(UnsafeCell);

lazy_just!(
    BorrowError,
    || {
        loop {
            let cell = RefCell::new(());
            let Ok(borrow_mut_guard) = cell.try_borrow_mut() else {
                continue;
            };
            if let Err(error) = cell.try_borrow() {
                drop(borrow_mut_guard);
                return error;
            }
        }
    };
    BorrowMutError,
    || {
        loop {
            let cell = RefCell::new(());
            let Ok(borrow_guard) = cell.try_borrow() else {
                continue;
            };
            if let Err(error) = cell.try_borrow_mut() {
                drop(borrow_guard);
                return error;
            }
        }
    }
);

#[cfg(test)]
mod test {
    use super::*;

    no_panic_test!(
        cell => Cell<u8>,
        ref_cell => RefCell<u8>,
        unsafe_cell => UnsafeCell<u8>
    );
}
