use std::mem;

#[path = "src/fatptr.rs"]
mod fatptr;

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
    let [data1, vtable1] = unsafe { fatptr::decomp(&instance1 as &dyn TestTrait) };
    let [data2, vtable2] = unsafe { fatptr::decomp(&instance2 as &dyn TestTrait) };

    assert_eq!(
        data1, &instance1 as *const Implementer1 as usize,
        "First part of the fat pointer does not point to the data"
    );

    let data1_vtable2: &dyn TestTrait = unsafe { &*fatptr::recomp([data1, vtable2]) };
    let data2_vtable1: &dyn TestTrait = unsafe { &*fatptr::recomp([data2, vtable1]) };
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
