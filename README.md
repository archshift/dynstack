# dynstack

## A stack for trait objects that minimizes allocations

### Usage

`dynstack` can mostly replace anywhere you'd use a stack, or a vector that doesn't
require removal from its center.

```rust
let mut stack = DynStack::<Debug>::new();
dyn_push!(stack, "hello, world!");
dyn_push!(stack, 0usize);
dyn_push!(stack, [1, 2, 3, 4, 5, 6]);

for item in stack.iter() {
    println!("{:?}", item);
}

// prints:
//  "hello, world!"
//  0
//  [1, 2, 3, 4, 5, 6]
```
