# Lesson 01 — Variables and Mutability

## Learning Objectives

By the end of this lesson, I should understand:

* What a variable binding is in Rust.
* How `let` creates an immutable binding by default.
* How `mut` enables reassignment.
* The difference between immutable and mutable bindings.
* Rust's type inference.
* How to explicitly declare a type.
* The difference between mutability and shadowing.
* Why immutability is an important software engineering concept.

---

## 1. Variable Bindings

Rust uses `let` to create a variable binding.

```rust
let name = "Nurdiansyah";
let age = 30;
```

In this example:

* `name` is a binding to the string `"Nurdiansyah"`.
* `age` is a binding to the integer `30`.

Rust variables are immutable by default.

This means that a binding created with `let` cannot be reassigned unless it is explicitly declared as mutable.

---

## 2. Immutable Bindings

The following code creates an immutable binding:

```rust
let age = 30;
```

Attempting to reassign it causes a compiler error:

```rust
let age = 30;

age = 31;
```

Rust rejects this because `age` is immutable.

The compiler prevents accidental modification of state.

This is one of Rust's important safety properties.

---

## 3. Mutable Bindings

A binding can be explicitly declared as mutable using the `mut` keyword:

```rust
let mut age = 30;

age = 31;
```

Now reassignment is allowed.

The important distinction is:

```rust
let age = 30;
```

means:

```text
Immutable binding
```

while:

```rust
let mut age = 30;
```

means:

```text
Mutable binding
```

Mutability must be explicitly requested.

---

## 4. Why Is Immutability the Default?

Immutable state makes software easier to reason about.

Consider:

```rust
let timeout = 30;
```

If `timeout` is immutable, other parts of the code cannot accidentally change its value through the same binding.

This provides several benefits:

* Reduces accidental state changes.
* Makes code easier to reason about.
* Makes assumptions easier to maintain.
* Helps the compiler detect programming mistakes.
* Encourages deliberate state management.

In production systems, predictable state is especially important for reliability and concurrency.

---

## 5. Type Inference

Rust is statically typed.

However, the programmer does not always need to explicitly specify the type.

For example:

```rust
let age = 30;
```

Rust can infer that `age` is an integer.

Similarly:

```rust
let name = "Nurdiansyah";
```

Rust can infer the type of `name`.

This feature is called type inference.

The compiler determines types during compilation while still enforcing Rust's static type system.

---

## 6. Explicit Types

A type can also be explicitly declared:

```rust
let age: u32 = 30;
let name: &str = "Nurdiansyah";
```

The syntax is:

```text
variable: Type
```

For example:

```rust
let age: u32 = 30;
```

means that `age` must have type `u32`.

Explicit types can be useful when:

* The compiler cannot infer the intended type.
* The type is important for readability.
* An API requires a specific type.
* We want to make an engineering constraint explicit.

---

## 7. Mutability vs. Shadowing

Rust also supports shadowing.

Example:

```rust
let x = 5;
let x = x + 1;
```

The second `let x` creates a new binding that shadows the previous binding.

This is different from mutation.

Mutation:

```rust
let mut x = 5;
x = x + 1;
```

Shadowing:

```rust
let x = 5;
let x = x + 1;
```

The distinction is important.

### Mutation

The same binding is changed:

```text
x → 5
x → 6
```

### Shadowing

A new binding is created:

```text
x → 5
new x → 6
```

Shadowing can also change the type:

```rust
let value = "42";
let value = value.parse::<u32>().unwrap();
```

The first `value` is a string slice.

The second `value` is a `u32`.

This is possible because shadowing creates a new binding.

---

## 8. Scope

A binding exists within a particular scope.

For example:

```rust
fn main() {
    let name = "Nurdiansyah";

    {
        let age = 30;

        println!("{}", age);
        println!("{}", name);
    }

    println!("{}", name);
}
```

`name` is available throughout the `main` function.

`age` only exists inside the inner block.

Once the inner block ends, `age` is no longer accessible.

Scope is fundamental to understanding Rust ownership and lifetime rules later in the curriculum.

---

## 9. Compiler Verification

Rust provides several useful Cargo commands.

### `cargo check`

Checks whether the code compiles without producing the final executable.

```bash
cargo check
```

This is useful during development because it provides fast compiler feedback.

### `cargo run`

Compiles and runs the program:

```bash
cargo run
```

### `cargo build`

Builds the project:

```bash
cargo build
```

The development workflow can therefore be:

```text
Edit
  ↓
cargo check
  ↓
Fix compiler errors
  ↓
cargo run
  ↓
Test behavior
```

---

## 10. Hands-on Lab

The laboratory project for this lesson is:

```text
hello-rust/
```

Initial implementation:

```rust
fn main() {
    let name = "Nurdiansyah";
    let age = 30;

    println!("Name: {}", name);
    println!("Age: {}", age);
}
```

Run:

```bash
cargo check
```

Then:

```bash
cargo run
```

Expected output:

```text
Name: Nurdiansyah
Age: 30
```

---

## 11. Deliberate Compiler Failure

To understand Rust's immutability rules, intentionally introduce an error:

```rust
fn main() {
    let name = "Nurdiansyah";
    let age = 30;

    age = 31;

    println!("Name: {}", name);
    println!("Age: {}", age);
}
```

Run:

```bash
cargo check
```

The compiler should reject the program because `age` was not declared as mutable.

Do not immediately fix the error.

First inspect the compiler diagnostic and understand:

1. Which line caused the error?
2. Which binding is immutable?
3. Why does Rust reject the assignment?
4. What does the compiler suggest?
5. How would `mut` change the program?

---

## 12. Correct Implementation

After understanding the compiler error, fix the program:

```rust
fn main() {
    let name = "Nurdiansyah";
    let mut age = 30;

    age = 31;

    println!("Name: {}", name);
    println!("Age: {}", age);
}
```

Verify:

```bash
cargo check
```

Then:

```bash
cargo run
```

Expected output:

```text
Name: Nurdiansyah
Age: 31
```

---

## 13. Engineering Takeaways

Rust's default immutability is a deliberate design choice.

Important concepts from this lesson:

```text
let
 ↓
immutable binding by default

let mut
 ↓
mutable binding

let x = ...
let x = ...
 ↓
shadowing / new binding
```

The distinction between mutation and shadowing is important because they represent different forms of state management.

Rust's compiler enforces these rules before the program executes.

This means many programming mistakes can be detected during compilation rather than at runtime.

---

## 14. Senior Engineering Perspective

At a senior engineering level, variables and mutability are not merely syntax.

They are part of state-management design.

When reviewing code, ask:

* Does this state actually need to be mutable?
* Can accidental mutation be prevented?
* Is shadowing improving clarity or hiding state changes?
* Is the chosen type appropriate?
* Is the scope as small as practical?
* Does the code make state transitions obvious?

The goal is not to avoid mutation completely.

The goal is to make state changes explicit, controlled, and understandable.

---

## Lesson Status

Status: Completed

Completed:

* [x] Understand immutable bindings.
* [x] Understand mutable bindings.
* [x] Understand `let`.
* [x] Understand `mut`.
* [x] Understand type inference.
* [x] Understand explicit types.
* [x] Understand shadowing.
* [x] Understand scope.
* [x] Complete hands-on lab.
* [x] Analyze compiler error.
* [x] Complete code review.

Next lesson:

`Lesson 02 — Data Types and Type System`
