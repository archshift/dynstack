/// Decompose a fat pointer into its constituent [pointer, extdata] pair
///
/// # Safety
///
/// Must only be called with the generic, `T`, being a trait object.
pub unsafe fn decomp<T: ?Sized>(ptr: *const T) -> [usize; 2] {
    let ptr_ref: *const *const T = &ptr;
    let decomp_ref = ptr_ref as *const [usize; 2];
    *decomp_ref
}

/// Recompose a fat pointer from its constituent [pointer, extdata] pair
///
/// # Safety
///
/// Must only be called with the generic, `T`, being a trait object.
pub unsafe fn recomp<T: ?Sized>(components: [usize; 2]) -> *mut T {
    let component_ref: *const [usize; 2] = &components;
    let ptr_ref = component_ref as *const *mut T;
    *ptr_ref
}