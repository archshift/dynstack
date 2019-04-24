use std::mem;

fn main() {
    // This build script sanity checks the memory layout of
    // trait objects/fat pointers. So if the Rust compiler ever change
    // layout of these, this crate should hopefully fail to compile
    // instead of producing programs that has undefined behavior.

    assert_eq!(
        mem::size_of::<&dyn TestTrait>(),
        mem::size_of::<[usize; 2]>(),
        "Trait objects does not have the expected size"
    );

    let instance1 = Implementer1(1);
    let instance2 = Implementer2(2);
    let [data1, vtable1] = unsafe { decomp_fat(&instance1 as &dyn TestTrait) };
    let [data2, vtable2] = unsafe { decomp_fat(&instance2 as &dyn TestTrait) };

    assert_eq!(
        data1, &instance1 as *const Implementer1 as usize,
        "First part of the fat pointer does not point to the data"
    );

    let data1_vtable2: &dyn TestTrait = unsafe { &*recomp_fat([data1, vtable2]) };
    let data2_vtable1: &dyn TestTrait = unsafe { &*recomp_fat([data2, vtable1]) };
    assert_eq!(
        data1_vtable2.calc(),
        1 + 20,
        "Recombining fat pointer from parts yielded unexpected result"
    );
    assert_eq!(
        data2_vtable1.calc(),
        2 + 10,
        "Recombining fat pointer from parts yielded unexpected result"
    );
}

trait TestTrait {
    fn calc(&self) -> u8;
}

struct Implementer1(u8);
impl TestTrait for Implementer1 {
    fn calc(&self) -> u8 {
        self.0 + 10
    }
}

struct Implementer2(u8);
impl TestTrait for Implementer2 {
    fn calc(&self) -> u8 {
        self.0 + 20
    }
}

/// Decompose a fat pointer into its constituent [pointer, extdata] pair
/// Keep in sync with the version in lib.rs
unsafe fn decomp_fat<T: ?Sized>(ptr: *const T) -> [usize; 2] {
    let ptr_ref: *const *const T = &ptr;
    let decomp_ref = ptr_ref as *const [usize; 2];
    *decomp_ref
}

/// Recompose a fat pointer from its constituent [pointer, extdata] pair
/// Keep in sync with the version in lib.rs
unsafe fn recomp_fat<T: ?Sized>(components: [usize; 2]) -> *const T {
    let component_ref: *const [usize; 2] = &components;
    let ptr_ref = component_ref as *const *const T;
    *ptr_ref
}
