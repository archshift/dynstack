//!
//! `dynstack` can mostly replace anywhere you'd use a stack, or a vector that doesn't
//! require removal from its center.
//!
//! ```
//! # use dynstack::{DynStack, dyn_push};
//! # use std::fmt::Debug;
//! let mut stack = DynStack::<dyn Debug>::new();
//! dyn_push!(stack, "hello, world!");
//! dyn_push!(stack, 0usize);
//! dyn_push!(stack, [1, 2, 3, 4, 5, 6]);
//!
//! for item in stack.iter() {
//!     println!("{:?}", item);
//! }
//!
//! // prints:
//! //  "hello, world!"
//! //  0
//! //  [1, 2, 3, 4, 5, 6]
//! ```

#![deny(rust_2018_idioms)]

use std::{
    alloc::{alloc, dealloc, Layout},
    marker::PhantomData,
    mem,
    ops::{Index, IndexMut},
    ptr,
};

/// Decompose a fat pointer into its constituent [pointer, extdata] pair
unsafe fn decomp_fat<T: ?Sized>(ptr: *const T) -> [usize; 2] {
    let ptr_ref: *const *const T = &ptr;
    let decomp_ref = ptr_ref as *const [usize; 2];
    *decomp_ref
}

/// Recompose a fat pointer from its constituent [pointer, extdata] pair
unsafe fn recomp_fat<T: ?Sized>(components: [usize; 2]) -> *const T {
    let component_ref: *const [usize; 2] = &components;
    let ptr_ref = component_ref as *const *const T;
    *ptr_ref
}

/// Recompose a mutable fat pointer from its constituent [pointer, extdata] pair
unsafe fn recomp_fat_mut<T: ?Sized>(components: [usize; 2]) -> *mut T {
    let component_ref: *const [usize; 2] = &components;
    let ptr_ref = component_ref as *const *mut T;
    *ptr_ref
}



/// Rounds up an integer to the nearest `align`
fn align_up(num: usize, align: usize) -> usize {
    let align_bits = align.trailing_zeros();
    (num + align - 1) >> align_bits << align_bits
}

#[test]
fn test_align_up() {
    let alignment = 4;
    let input = &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9];
    let expected = &[0, 4, 4, 4, 4, 8, 8, 8, 8, 12];
    let output = input.iter().map(|x| align_up(*x, alignment));
    let both = expected.iter().zip(output);
    for (l, r) in both {
        assert_eq!(*l, r);
    }
}



/// Iterator over trait object references
pub struct DynStackIter<'a, T: ?Sized> {
    stack: &'a DynStack<T>,
    index: usize,
}

impl<'a, T: 'a + ?Sized> Iterator for DynStackIter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<&'a T> {
        self.stack.get(self.index)
            .map(|out| {self.index += 1; out})
    }
}


/// Iterator over mutable trait object references
pub struct DynStackIterMut<'a, T: ?Sized> {
    stack: *mut DynStack<T>,
    index: usize,
    _spooky: PhantomData<&'a mut DynStack<T>>
}

impl<'a, T: 'a + ?Sized> Iterator for DynStackIterMut<'a, T> {
    type Item = &'a mut T;
    fn next(&mut self) -> Option<&'a mut T> {
        unsafe {
            (*self.stack).get_mut(self.index)
                .map(|out| {self.index += 1; out})
        }
    }
}




pub struct DynStack<T: ?Sized> {
    offs_table: Vec<(usize, usize)>,
    dyn_data: *mut u8,
    dyn_size: usize,
    dyn_cap: usize,
    max_align: usize,
    _spooky: PhantomData<T>,
}

impl<T: ?Sized> DynStack<T> {
    fn make_layout(cap: usize) -> Layout {
        unsafe { Layout::from_size_align_unchecked(cap, 16) }
    }
    fn layout(&self) -> Layout {
        Self::make_layout(self.dyn_cap)
    }

    /// Creates a new, empty, [`DynStack`].
    ///
    /// # Panics
    ///
    /// Panics if `T` is not a trait object.
    pub fn new() -> Self {
        assert_eq!(
            mem::size_of::<*const T>(),
            mem::size_of::<[usize; 2]>(),
            "Used on non trait object!"
        );
        Self {
            offs_table: Vec::new(),
            dyn_data: ptr::null_mut(),
            dyn_size: 0,
            dyn_cap: 0,
            max_align: 16,
            _spooky: PhantomData
        }
    }

    /// Called on first push to allocate heap data.
    /// `DynStack::new` does not perform any allocation,
    /// since it makes creating `DynStack` instances a lot faster.
    fn allocate(&mut self, item_size: usize) {
        // Always allocate a power of two size, fitting the first item.
        // At least 16 bytes.
        let alloc_size = item_size.next_power_of_two().max(16);
        self.dyn_cap = alloc_size;
        self.dyn_data = unsafe { alloc(Self::make_layout(alloc_size)) };
    }

    #[cfg(test)]
    fn reallocate(&mut self, new_cap: usize) {
        let old_layout = self.layout();
        self.dyn_cap = new_cap;
        unsafe {
            // The point of this is to maximize the chances of having changed alignment
            // characteristics, for testing purposes.
            let new_data = alloc(self.layout());
            ptr::copy_nonoverlapping(self.dyn_data, new_data, self.dyn_size);
            dealloc(self.dyn_data, old_layout);
            self.dyn_data = new_data;
        }
    }

    #[cfg(not(test))]
    fn reallocate(&mut self, new_cap: usize) {
        use std::alloc::realloc;
        self.dyn_cap = new_cap;
        self.dyn_data = unsafe { realloc(self.dyn_data, self.layout(), self.dyn_cap) };
    }

    /// Double the stack's capacity
    fn grow(&mut self) {
        let align_mask = self.max_align - 1;
        let prev_align = self.dyn_data as usize & align_mask;

        let new_cap = self.dyn_cap * 2;
        self.reallocate(new_cap);

        let new_align = self.dyn_data as usize & align_mask;
        let mut align_diff = (new_align as isize) - (prev_align as isize);

        if align_diff != 0 {
            // It's possible that, if we have an item with alignment > 16, it becomes unaligned when
            // reallocating our buffer (since we realloc with the default alignment of 16).
            // If that happens, we need to realign all of our buffer contents with a memmove and adjust the
            // offset table appropriately.

            let first_offset = self.offs_table[0].0 as isize;
            if align_diff > 0 || first_offset + align_diff < 0 {
                // Not enough padding at the start of the buf; must move foreward to align
                align_diff = ((align_diff as usize) & align_mask) as isize;
            }

            unsafe {
                let start_ptr = self.dyn_data.offset(first_offset);
                let dst = start_ptr.offset(align_diff);
                debug_assert!(dst as usize >= self.dyn_data as usize);
                debug_assert!(dst as usize <= (self.dyn_data as usize) + self.dyn_cap);
                ptr::copy(start_ptr, dst, self.dyn_size);
            }
            for (ref mut offs, _) in &mut self.offs_table {
                *offs = offs.wrapping_add(align_diff as usize);
            }
        }
    }

    /// Push a trait object onto the stack.
    ///
    /// This method is unsafe because in lieu of moving a trait object onto `push`'s stack
    /// (not possible in rust as of 1.30.0) we copy it from the provided mutable pointer.
    ///
    /// The user of this method must therefore either ensure that `item` has no `Drop` impl,
    /// or explicitly call `std::mem::forget` on `item` after pushing.
    ///
    /// It is highly recommended to use the `dyn_push` macro instead of calling this directly.
    pub unsafe fn push(&mut self, item: *mut T) {
        let size = mem::size_of_val(&*item);
        let align = mem::align_of_val(&*item);

        // If we have not yet allocated any data, start by doing so.
        if self.dyn_data.is_null() {
            self.allocate(size);
        }

        let align_offs = loop {
            let curr_ptr = self.dyn_data as usize + self.dyn_size;
            let aligned_ptr = align_up(curr_ptr, align);
            let align_offs = aligned_ptr - curr_ptr;

            if self.dyn_size + align_offs + size > self.dyn_cap {
                self.grow();
            } else {
                break align_offs;
            }
        };

        self.dyn_data
            .add(self.dyn_size)
            .add(align_offs)
            .copy_from_nonoverlapping(item as *const u8, size);

        let ptr_components = decomp_fat(item);
        self.offs_table.push((self.dyn_size + align_offs, ptr_components[1]));

        self.dyn_size += align_offs + size;
        self.max_align = align.max(self.max_align);
    }

    /// Remove the last trait object from the stack.
    /// Returns true if any items were removed.
    pub fn remove_last(&mut self) -> bool {
        if let Some(last_item) = self.peek_mut() {
            unsafe { ptr::drop_in_place(last_item) };
        } else {
            return false
        }
        let (last_offs, _) = self.offs_table.pop().unwrap();
        self.dyn_size = last_offs;
        true
    }

    /// Retrieve a trait object reference at the provided index.
    pub fn get<'a>(&'a self, index: usize) -> Option<&'a T> {
        let item = self.offs_table.get(index)?;
        let components = [self.dyn_data as usize + item.0, item.1];
        let out = unsafe { &*recomp_fat(components) };
        Some(out)
    }

    /// Retrieve a mutable trait object reference at the provided index.
    pub fn get_mut<'a>(&'a mut self, index: usize) -> Option<&'a mut T> {
        let item = self.offs_table.get(index)?;
        let components = [self.dyn_data as usize + item.0, item.1];
        let out = unsafe { &mut *recomp_fat_mut(components) };
        Some(out)
    }

    /// Retrieve the trait object reference at the top of the stack.
    pub fn peek<'a>(&'a self) -> Option<&'a T> {
        self.get(self.len().wrapping_sub(1))
    }

    /// Retrieve the mutable trait object reference at the top of the stack.
    pub fn peek_mut<'a>(&'a mut self) -> Option<&'a mut T> {
        let index = self.len().wrapping_sub(1);
        self.get_mut(index)
    }

    /// Returns the number of trait objects stored on the stack.
    pub fn len(&self) -> usize {
        self.offs_table.len()
    }
}

impl<'a, T: 'a + ?Sized> DynStack<T> {
    /// Returns an iterator over trait object references
    pub fn iter(&'a self) -> DynStackIter<'a, T> {
        DynStackIter {
            stack: self,
            index: 0
        }
    }

    /// Returns an iterator over mutable trait object references
    pub fn iter_mut(&'a mut self) -> DynStackIterMut<'a, T> {
        DynStackIterMut {
            stack: self,
            index: 0,
            _spooky: PhantomData
        }
    }
}


impl<T: ?Sized> Index<usize> for DynStack<T> {
    type Output = T;

    fn index(&self, idx: usize) -> &T {
        self.get(idx).unwrap()
    }
}

impl<T: ?Sized> IndexMut<usize> for DynStack<T> {
    fn index_mut(&mut self, idx: usize) -> &mut T {
        self.get_mut(idx).unwrap()
    }
}


impl<'a, T: 'a + ?Sized> IntoIterator for &'a DynStack<T> {
    type Item = &'a T;
    type IntoIter = DynStackIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T: 'a + ?Sized> IntoIterator for &'a mut DynStack<T> {
    type Item = &'a mut T;
    type IntoIter = DynStackIterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}


impl<T: ?Sized> Drop for DynStack<T> {
    fn drop(&mut self) {
        while self.remove_last() {}
        unsafe { dealloc(self.dyn_data, self.layout()) }
    }
}



/// Push an item onto the back of the specified stack
#[macro_export]
macro_rules! dyn_push {
    { $stack:expr, $item:expr } => {{
        let mut t = $item;

        unsafe { $stack.push(&mut t) };
        ::std::mem::forget(t);
    }}
}



#[test]
fn test_push_pop() {
    use std::fmt::Debug;
    let mut stack = DynStack::<dyn Debug>::new();
    let bunch = vec![1u32, 2, 3, 4, 5, 6, 7, 8, 9];
    dyn_push!(stack, 1u8);
    dyn_push!(stack, 1u32);
    dyn_push!(stack, 1u16);
    dyn_push!(stack, 1u64);
    dyn_push!(stack, 1u128);
    dyn_push!(stack, bunch);
    dyn_push!(stack, { #[derive(Debug)] struct ZST; ZST });

    if let Some(item) = stack.peek() {
        println!("{:?}", item);
        assert!(format!("{:?}", item) == "ZST");
    } else {
        unreachable!();
    }
    assert!( stack.remove_last() );

    if let Some(item) = stack.peek() {
        println!("{:?}", item);
        assert!(format!("{:?}", item) == "[1, 2, 3, 4, 5, 6, 7, 8, 9]");
    }
    assert!( stack.remove_last() );

    for _i in 0..5 {
        if let Some(item) = stack.peek() {
            println!("{:?}", item);
            assert!(format!("{:?}", item) == "1");
        } else {
            unreachable!();
        }
        assert!( stack.remove_last() );
    }

    assert!( stack.len() == 0 );
    assert!( stack.dyn_size == 0 );
}

#[test]
fn test_fn() {
    let mut stack = DynStack::<dyn Fn() -> usize>::new();
    for i in 0..100 {
        dyn_push!(stack, move || i);
    }

    let mut item2 = 0;
    for func in stack.iter() {
        item2 += func();
    }
    assert_eq!(item2, 4950);
}

#[test]
fn test_drop() {
    use std::any::Any;
    use std::collections::HashSet;

    static mut DROP_NUM: Option<HashSet<usize>> = None;
    unsafe { DROP_NUM = Some(HashSet::new()) };
    fn drop_num() -> &'static HashSet<usize> { unsafe { DROP_NUM.as_ref().unwrap() } }
    fn drop_num_mut() -> &'static mut HashSet<usize> { unsafe { DROP_NUM.as_mut().unwrap() } }


    struct Droppable {counter: usize};
    impl Drop for Droppable {
        fn drop(&mut self) {
            drop_num_mut().insert(self.counter);
        }
    }

    {
        let mut stack = DynStack::<dyn Any>::new();
        dyn_push!(stack, Droppable{counter: 1});
        dyn_push!(stack, Droppable{counter: 2});
        dyn_push!(stack, Droppable{counter: 3});
        dyn_push!(stack, Droppable{counter: 4});
        dyn_push!(stack, Droppable{counter: 5});
        dyn_push!(stack, Droppable{counter: 6});
        assert!(drop_num().is_empty());
    }

    let expected: HashSet<usize> = [1, 2, 3, 4, 5, 6].iter().cloned().collect();
    assert_eq!(drop_num(), &expected);
}

#[test]
fn test_align() {
    trait Aligned {
        fn alignment(&self) -> usize;
    }
    impl Aligned for u32 {
        fn alignment(&self) -> usize { ::std::mem::align_of::<Self>() }
    }
    impl Aligned for u64 {
        fn alignment(&self) -> usize { ::std::mem::align_of::<Self>() }
    }

    #[repr(align(32))]
    struct Aligned32 {
        _dat: [u8; 32]
    }
    impl Aligned for Aligned32 {
        fn alignment(&self) -> usize { ::std::mem::align_of::<Self>() }
    }

    #[repr(align(64))]
    struct Aligned64 {
        _dat: [u8; 64]
    }
    impl Aligned for Aligned64 {
        fn alignment(&self) -> usize { ::std::mem::align_of::<Self>() }
    }

    fn new32() -> Aligned32 {
        let mut dat = [0u8; 32];
        for i in 0..32 {
            dat[i] = i as u8;
        }
        Aligned32 { _dat: dat }
    }
    fn new64() -> Aligned64 {
        let mut dat = [0u8; 64];
        for i in 0..64 {
            dat[i] = i as u8;
        }
        Aligned64 { _dat: dat }
    }

    let assert_aligned = |item: &dyn Aligned| {
        let thin_ptr = item as *const dyn Aligned as *const () as usize;
        println!("item expects alignment {}, got offset {}", item.alignment(),
            thin_ptr & (item.alignment() - 1));
        assert!(thin_ptr & (item.alignment() - 1) == 0);
    };

    let mut stack = DynStack::<dyn Aligned>::new();

    dyn_push!(stack, new32());
    dyn_push!(stack, new64());
    assert_aligned(stack.peek().unwrap());

    for i in 0..256usize {
        let randomized = (i.pow(7) % 13) % 4;
        match randomized {
            0 => dyn_push!(stack, 0xF0B0D0E0u32),
            1 => dyn_push!(stack, 0x01020304F0B0D0E0u64),
            2 => dyn_push!(stack, new32()),
            3 => dyn_push!(stack, new64()),
            _ => unreachable!()
        }
        assert_aligned(stack.peek().unwrap());
    }
}

#[test]
#[should_panic]
fn test_non_dyn() {
    let _stack: DynStack<u8> = DynStack::new();
}
