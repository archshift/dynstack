#[macro_use]
extern crate criterion;
extern crate dynstack;

use criterion::Bencher;
use criterion::Criterion;

use dynstack::{DynStack, dyn_push};

use std::fmt::Display;

fn new_speed_naive(b: &mut Bencher) {
    b.iter(|| {
        Vec::<Box<dyn Display>>::new()
    });
}

fn new_speed_dynstack(b: &mut Bencher) {
    b.iter(|| {
        DynStack::<dyn Display>::new()
    });
}

fn push_speed_naive(b: &mut Bencher) {
    let mut vec = Vec::<Box<Display>>::new();
    b.iter(|| {
        vec.push(Box::new(0xF00BAAusize));
        vec.push(Box::new(0xABBAu16));
        vec.push(Box::new(0xBA7123AAu32));
        vec.push(Box::new(12u8));
    });
}

fn push_speed_dynstack(b: &mut Bencher) {
    let mut stack = DynStack::<Display>::new();
    b.iter(|| {
        dyn_push!(stack, 0xF00BAAusize);
        dyn_push!(stack, 0xABBAu16);
        dyn_push!(stack, 0xBA7123AAu32);
        dyn_push!(stack, 12u8);
    });
}

fn push_and_run_naive(b: &mut Bencher) {
    b.iter(|| {
        let mut stack = Vec::<Box<Fn() -> usize>>::new();
        fn pseudorecursive(stack: &mut Vec<Box<Fn() -> usize>>, n: usize) {
            stack.push(Box::new(move || n - 1));
        }

        let mut n = 100;
        let mut i = 0;
        while n > 0 {
            pseudorecursive(&mut stack, n);
            n = (stack.get(i).unwrap())();
            i += 1;
        }
    });
}

fn push_and_run_dynstack(b: &mut Bencher) {
    b.iter(|| {
        let mut stack = DynStack::<Fn() -> usize>::new();
        fn pseudorecursive(stack: &mut DynStack<Fn() -> usize>, n: usize) {
            dyn_push!(stack, move || n - 1);
        }

        let mut n = 100;
        let mut i = 0;
        while n > 0 {
            pseudorecursive(&mut stack, n);
            n = (stack.get(i).unwrap())();
            i += 1;
        }
    });
}

fn pseudorecursive2_naive(b: &mut Bencher) {
    b.iter(|| {
        let mut state: Box<Fn() -> usize> = Box::new(|| 0);
        fn pseudorecursive(state: &mut Box<Fn() -> usize>, n: usize) {
            *state = Box::new(move || n - 1);
        }

        let mut n = 100;
        while n > 0 {
            pseudorecursive(&mut state, n);
            n = state();
        }
    });
}

fn pseudorecursive2_dynstack(b: &mut Bencher) {
    b.iter(|| {
        let mut stack = DynStack::<Fn() -> usize>::new();
        fn pseudorecursive(stack: &mut DynStack<Fn() -> usize>, n: usize) {
            dyn_push!(stack, move || n - 1);
        }

        let mut n = 100;
        while n > 0 {
            pseudorecursive(&mut stack, n);
            n = (stack.peek().unwrap())();
            stack.remove_last();
        }
    });
}

fn criterion_benchmark(c: &mut Criterion) {
    c.bench_function("new_speed_naive", new_speed_naive);
    c.bench_function("new_speed_dynstack", new_speed_dynstack);
    c.bench_function("push_speed_naive", push_speed_naive);
    c.bench_function("push_speed_dynstack", push_speed_dynstack);
    c.bench_function("push_and_run_naive", push_and_run_naive);
    c.bench_function("push_and_run_dynstack", push_and_run_dynstack);
    c.bench_function("pseudorecursive2_naive", pseudorecursive2_naive);
    c.bench_function("pseudorecursive2_dynstack", pseudorecursive2_dynstack);
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
