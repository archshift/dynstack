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
pub struct DynVecIter<'a, T: 'a + ?Sized> {
    vec: &'a DynVec<T>,
    index: usize,
}

impl<'a, T: 'a + ?Sized> Iterator for DynVecIter<'a, T> {
    type Item = &'a T;
    fn next(&mut self) -> Option<&'a T> {
        self.vec.get(self.index)
            .map(|out| {self.index += 1; out})
    }
}


/// Iterator over mutable trait object references
pub struct DynVecIterMut<'a, T: 'a + ?Sized> {
    vec: *mut DynVec<T>,
    index: usize,
    _spooky: PhantomData<&'a mut DynVec<T>>
}

impl<'a, T: 'a + ?Sized> Iterator for DynVecIterMut<'a, T> {
    type Item = &'a mut T;
    fn next(&mut self) -> Option<&'a mut T> {
        unsafe {
            (*self.vec).get_mut(self.index)
                .map(|out| {self.index += 1; out})
        }
    }
}



pub struct DynVec<T: ?Sized> {
    offs_table: Vec<(usize, usize)>,
    dyn_data: *mut u8,
    dyn_size: usize,
    dyn_cap: usize,
    _spooky: PhantomData<T>,
}

impl<T: ?Sized> DynVec<T> {
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
    
    /// Double the vector's capacity
    fn grow(&mut self) {
        self.dyn_cap *= 2;
        self.dyn_data = unsafe { realloc(self.dyn_data, Self::base_layout(), self.dyn_cap) };
    }

    /// Push a trait object onto the vec.
    ///
    /// This method is unsafe because in lieu of moving a trait object onto `push`'s stack
    /// (not possible in rust as of 1.30.0) we copy it from the provided mutable reference.
    /// 
    /// The user of this method must therefore ensure that `item` either has no `Drop` impl,
    /// or explicitly call `std::mem::forget` on `item` after pushing.
    ///
    /// It is highly recommended to use the `dyn_push` macro instead of calling this directly.
    pub unsafe fn push(&mut self, item: &mut T) {
        let size = mem::size_of_val(item);
        let align = mem::align_of_val(item);

        let curr_ptr = self.dyn_data as usize + self.dyn_size;
        let aligned_ptr = align_up(curr_ptr, align);
        let align_offs = aligned_ptr - curr_ptr;

        while self.dyn_size + align_offs + size > self.dyn_cap {
            self.grow();
        }
        self.dyn_data
            .add(self.dyn_size)
            .add(align_offs)
            .copy_from_nonoverlapping(item as *const T as *const u8, size);
        
        let ptr_components = decomp_fat(item);
        self.offs_table.push((self.dyn_size + align_offs, ptr_components[1]));
         
        self.dyn_size += align_offs + size;
    }

    /// Retrieve a trait object reference at the provided index.
    pub fn get<'a>(&'a self, index: usize) -> Option<&'a T> {
        if let Some(item) = self.offs_table.get(index) {
            let components = [self.dyn_data as usize + item.0, item.1];
            let out = unsafe { &*recomp_fat(components) };
            Some(out)
        } else {
            None
        }
    }

    /// Retrieve a mutable trait object reference at the provided index.
    pub fn get_mut<'a>(&'a mut self, index: usize) -> Option<&'a mut T> {
        if let Some(item) = self.offs_table.get(index) {
            let components = [self.dyn_data as usize + item.0, item.1];
            let out = unsafe { &mut *recomp_fat_mut(components) };
            Some(out)
        } else {
            None
        }
    }
}

impl<'a, T: 'a + ?Sized> DynVec<T> {
    /// Returns an iterator over trait object references
    fn iter(&'a self) -> DynVecIter<'a, T> {
        DynVecIter {
            vec: self,
            index: 0
        }
    }

    /// Returns an iterator over mutable trait object references
    fn iter_mut(&'a mut self) -> DynVecIterMut<'a, T> {
        DynVecIterMut {
            vec: self,
            index: 0,
            _spooky: PhantomData
        }
    }
}


impl<'a, T: 'a + ?Sized> IntoIterator for &'a DynVec<T> {
    type Item = &'a T;
    type IntoIter = DynVecIter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a, T: 'a + ?Sized> IntoIterator for &'a mut DynVec<T> {
    type Item = &'a mut T;
    type IntoIter = DynVecIterMut<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}


impl<T: ?Sized> Drop for DynVec<T> {
    fn drop(&mut self) {
        for item in self.iter_mut() {
            unsafe { ptr::drop_in_place(item) };
        }

        unsafe { dealloc(self.dyn_data, Self::base_layout()) }
    }
}



#[macro_export]
macro_rules! dyn_push {
    { $vec:expr, $item:expr } => {{
        let mut t = $item;

        unsafe { $vec.push(&mut t) };
        ::std::mem::forget(t);
    }}
}



#[test]
fn test_push_get() {
    use std::fmt::Debug;
    let mut vec = DynVec::<Debug>::new();
    let bunch = vec![1u32, 2, 3, 4, 5, 6, 7, 8, 9];
    dyn_push!(vec, 1u8);
    dyn_push!(vec, 1u32);
    dyn_push!(vec, 1u16);
    dyn_push!(vec, 1u64);
    dyn_push!(vec, bunch);
    
    for i in 0..4 {
        println!("{:?}", vec.get(i).unwrap());
        assert!(format!("{:?}", vec.get(i).unwrap()) == "1");
    }

    println!("{:?}", vec.get(4).unwrap());
    assert!(format!("{:?}", vec.get(4).unwrap()) == "[1, 2, 3, 4, 5, 6, 7, 8, 9]");
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
        let mut vec = DynVec::<Any>::new();
        dyn_push!(vec, Droppable{counter: 1});
        dyn_push!(vec, Droppable{counter: 2});
        dyn_push!(vec, Droppable{counter: 3});
        dyn_push!(vec, Droppable{counter: 4});
        dyn_push!(vec, Droppable{counter: 5});
        dyn_push!(vec, Droppable{counter: 6});
        assert!(drop_num().is_empty());
    }

    let expected: HashSet<usize> = [1, 2, 3, 4, 5, 6].iter().cloned().collect();
    assert_eq!(drop_num(), &expected);
}
