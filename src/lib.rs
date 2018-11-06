use std::alloc::{alloc, dealloc, realloc, Layout};
use std::mem;
use std::marker::PhantomData;
use std::ptr;



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
pub struct DynStackIter<'a, T: 'a + ?Sized> {
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
pub struct DynStackIterMut<'a, T: 'a + ?Sized> {
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
    _spooky: PhantomData<T>,
}

impl<T: ?Sized> DynStack<T> {
    fn base_layout() -> Layout {
        unsafe { Layout::from_size_align_unchecked(16, 16) }
    }

    pub fn new() -> Self {
        Self {
            offs_table: Vec::new(),
            dyn_data: unsafe { alloc(Self::base_layout()) },
            dyn_size: 0,
            dyn_cap: 16,
            _spooky: PhantomData
        }
    }
    
    /// Double the stack's capacity
    fn grow(&mut self) {
        self.dyn_cap *= 2;
        self.dyn_data = unsafe { realloc(self.dyn_data, Self::base_layout(), self.dyn_cap) };
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

        let curr_ptr = self.dyn_data as usize + self.dyn_size;
        let aligned_ptr = align_up(curr_ptr, align);
        let align_offs = aligned_ptr - curr_ptr;

        while self.dyn_size + align_offs + size > self.dyn_cap {
            self.grow();
        }
        self.dyn_data
            .add(self.dyn_size)
            .add(align_offs)
            .copy_from_nonoverlapping(item as *const u8, size);
        
        let ptr_components = decomp_fat(item);
        self.offs_table.push((self.dyn_size + align_offs, ptr_components[1]));
         
        self.dyn_size += align_offs + size;
    }

    /// Remove the last trait object from the stack.
    /// Returns true if any items were removed.
    pub fn remove_last(&mut self) -> bool {
        if let Some((last_offs, _)) = self.offs_table.pop() {
            self.dyn_size = last_offs;
            true
        } else {
            false
        }
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
        self.get(self.len() - 1)
    }

    /// Retrieve the mutable trait object reference at the top of the stack.
    pub fn peek_mut<'a>(&'a mut self) -> Option<&'a mut T> {
        let index = self.len() - 1;
        self.get_mut(index)
    }

    /// Returns the number of trait objects stored on the stack.
    pub fn len(&self) -> usize {
        self.offs_table.len()
    }
}

impl<'a, T: 'a + ?Sized> DynStack<T> {
    /// Returns an iterator over trait object references
    fn iter(&'a self) -> DynStackIter<'a, T> {
        DynStackIter {
            stack: self,
            index: 0
        }
    }

    /// Returns an iterator over mutable trait object references
    fn iter_mut(&'a mut self) -> DynStackIterMut<'a, T> {
        DynStackIterMut {
            stack: self,
            index: 0,
            _spooky: PhantomData
        }
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
        for item in self.iter_mut() {
            unsafe { ptr::drop_in_place(item) };
        }

        unsafe { dealloc(self.dyn_data, Self::base_layout()) }
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
    let mut stack = DynStack::<Debug>::new();
    let bunch = vec![1u32, 2, 3, 4, 5, 6, 7, 8, 9];
    dyn_push!(stack, 1u8);
    dyn_push!(stack, 1u32);
    dyn_push!(stack, 1u16);
    dyn_push!(stack, 1u64);
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

    for _i in 0..4 {
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
    let mut stack = DynStack::<Fn() -> usize>::new();
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
        let mut stack = DynStack::<Any>::new();
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
