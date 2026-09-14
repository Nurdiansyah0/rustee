# Lesson 02 — Data Types and Type System

## Learning Objectives

By the end of this lesson, I should understand:

* What a data type is in Rust.
* Why programming languages use data types.
* Rust's static type system.
* Rust's type inference.
* How to explicitly declare a type.
* The difference between scalar and compound types.
* Rust's integer types.
* The difference between signed and unsigned integers.
* Floating-point types.
* The `bool` and `char` types.
* Tuples and arrays.
* Basic type conversion.
* How Rust detects type mismatches during compilation.
* Why choosing an appropriate data type is an important software engineering concept.

---

## 1. Data Types

A data type describes what kind of value a variable represents.

For example:

```rust
let age = 30;
let active = true;
let temperature = 36.5;
```

In this example:

* `age` represents an integer value.
* `active` represents a boolean value.
* `temperature` represents a floating-point value.

The data type determines what kind of value can be stored and what operations can be performed on that value.

Data types are therefore an important part of program correctness.

---

## 2. Static Type System

Rust is a statically typed language.

This means that types are checked during compilation.

For example:

```rust
let age: i32 = 30;
```

The compiler knows that `age` has type `i32`.

If an incompatible value is assigned:

```rust
let age: i32 = true;
```

Rust rejects the program during compilation.

The compiler detects the type mismatch before the program runs.

This is an important part of Rust's safety properties.

---

## 3. Type Inference

The programmer does not always need to explicitly specify the type.

For example:

```rust
let age = 30;
```

Rust can infer the type of `age`.

Similarly:

```rust
let active = true;
```

Rust can determine that `active` is a boolean.

This feature is called type inference.

The compiler determines types during compilation while still enforcing Rust's static type system.

---

## 4. Explicit Types

A type can also be explicitly declared:

```rust
let age: u32 = 30;
let active: bool = true;
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

## 5. Scalar Types

Rust has four primary scalar types:

* Integer types.
* Floating-point types.
* Boolean type.
* Character type.

Examples:

```rust
let age: u32 = 30;
let temperature: f64 = 36.5;
let active: bool = true;
let initial: char = 'N';
```

A scalar type represents a single value.

### Integer Types

Rust provides several integer types:

```text
i8
i16
i32
i64
i128
isize

u8
u16
u32
u64
u128
usize
```

The naming convention is:

```text
i → signed integer
u → unsigned integer
```

The number represents the number of bits.

For example:

```text
i32
```

is a signed 32-bit integer.

### Signed and Unsigned Integers

Signed integers can represent negative and positive values:

```rust
let temperature: i32 = -10;
```

Unsigned integers cannot represent negative values:

```rust
let quantity: u32 = 10;
```

This is invalid:

```rust
let quantity: u32 = -10;
```

because `u32` cannot represent negative values.

For example:

```text
i8
→ -128 through 127

u8
→ 0 through 255
```

The appropriate integer type depends on the domain of the value.

### Floating-Point Types

Rust provides:

```text
f32
f64
```

For example:

```rust
let temperature: f64 = 36.5;
```

Floating-point types are useful when a value contains a fractional component.

`f64` generally provides more precision than `f32`.

However, floating-point arithmetic does not represent every decimal value exactly.

For this reason, financial applications should carefully consider whether floating-point values are appropriate.

### Boolean Type

The boolean type is:

```rust
bool
```

It has two possible values:

```text
true
false
```

For example:

```rust
let active: bool = true;
```

### Character Type

The character type is:

```rust
char
```

A character is written using single quotes:

```rust
let initial: char = 'N';
```

Rust's `char` represents a Unicode scalar value.

A character is different from a string:

```rust
'A'
```

is a `char`.

While:

```rust
"A"
```

is a string slice.

They are different types.

---

## 6. Compound Types

Compound types group multiple values into a single value.

Two fundamental compound types in Rust are:

* Tuples.
* Arrays.

### Tuples

A tuple can contain multiple values with different types:

```rust
let user = ("Nurdiansyah", 30, true);
```

In this example:

* `"Nurdiansyah"` is text.
* `30` is an integer.
* `true` is a boolean.

Tuple elements can be accessed using an index:

```rust
let user = ("Nurdiansyah", 30, true);

println!("{}", user.0);
println!("{}", user.1);
println!("{}", user.2);
```

Tuple indexing starts at zero.

### Arrays

An array contains multiple values of the same type.

For example:

```rust
let numbers = [10, 20, 30, 40];
```

All elements must have the same type.

An array also has a fixed length.

The type of this array is:

```text
[i32; 4]
```

This means:

```text
i32
→ element type

4
→ number of elements
```

Therefore:

```rust
let numbers: [i32; 4] = [10, 20, 30, 40];
```

Array elements can be accessed using an index:

```rust
println!("{}", numbers[0]);
println!("{}", numbers[1]);
```

The first element has index `0`.

---

## 7. Type Conversion

Different numeric types are not automatically treated as the same type.

For example:

```rust
let amount: i32 = 1000;
let balance: i64 = 5000;
```

`i32` and `i64` are different types.

An explicit conversion can be performed:

```rust
let amount: i32 = 1000;
let amount_i64 = amount as i64;
```

Now:

```text
amount
→ i32

amount_i64
→ i64
```

The conversion is explicit.

This is important because changing a numeric type can affect:

* Range.
* Precision.
* Sign.
* Overflow.
* Representation.

A conversion that compiles is not automatically a conversion that is correct for the application.

---

## 8. Type Mismatch

Rust does not automatically treat unrelated types as interchangeable.

For example:

```rust
let age: i32 = 30;
let active: bool = true;
```

This is valid because each value matches its declared type.

But:

```rust
let age: i32 = true;
```

is invalid.

The compiler will report a type mismatch.

Conceptually:

```text
expected:
i32

found:
bool
```

The compiler detects this before the program runs.

This allows programming mistakes to be identified during development.

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

### `cargo fmt`

Formats the Rust source code:

```bash
cargo fmt
```

### `cargo clippy`

Runs Rust's Clippy linter:

```bash
cargo clippy -- -D warnings
```

The development workflow can therefore be:

```text
Edit
  ↓
cargo check
  ↓
Fix compiler errors
  ↓
cargo fmt
  ↓
cargo clippy
  ↓
cargo run
  ↓
Test behavior
```

---

## 10. Hands-on Lab

The laboratory project for this lesson is:

```text
labs/data-types/
```

Create the project:

```bash
cargo new labs/data-types
```

Then enter the project:

```bash
cd labs/data-types
```

Initial implementation:

```rust
fn main() {
    let age: u32 = 30;
    let temperature: f64 = 36.5;
    let active: bool = true;
    let initial: char = 'N';

    println!("Age: {}", age);
    println!("Temperature: {}", temperature);
    println!("Active: {}", active);
    println!("Initial: {}", initial);
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
Age: 30
Temperature: 36.5
Active: true
Initial: N
```

Extend the program by adding:

* A person's name.
* A tuple containing name, age, and active status.
* An array containing five transaction amounts.

Use appropriate data types.

---

## 11. Deliberate Compiler Failure

To understand Rust's type checking, intentionally introduce an error:

```rust
fn main() {
    let age: i32 = true;

    println!("Age: {}", age);
}
```

Run:

```bash
cargo check
```

The compiler should reject the program because `true` is a boolean while `age` was declared as an `i32`.

Do not immediately fix the error.

First inspect the compiler diagnostic and understand:

1. Which line caused the error?
2. What type did the compiler expect?
3. What type did the compiler find?
4. Why are the two types incompatible?
5. How would you correct the program?

---

## 12. Correct Implementation

After understanding the compiler error, fix the program:

```rust
fn main() {
    let age: i32 = 30;

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
Age: 30
```

---

## 13. Engineering Takeaways

Rust's type system is a fundamental part of program correctness.

Important concepts from this lesson:

```text
Data Type
    ↓
describes the kind of value

Static Type System
    ↓
types are checked during compilation

Type Inference
    ↓
Rust determines types automatically when possible

Explicit Type
    ↓
programmer declares the intended type

Scalar Type
    ↓
single value

Compound Type
    ↓
multiple values
```

The primary scalar types covered in this lesson are:

```text
Integer
Floating-point
bool
char
```

The primary compound types covered are:

```text
Tuple
Array
```

Different numeric types are not automatically interchangeable.

Explicit conversion should be intentional.

---

## 14. Senior Engineering Perspective

At a senior engineering level, data types are not merely syntax.

They are part of domain modeling.

When reviewing a type choice, ask:

* What does this value represent?
* Can it be negative?
* What range must it support?
* Does it require exact precision?
* Could it overflow?
* How is it represented in the database?
* How is it transferred through an API?
* What happens when the value exceeds its expected range?

For example:

```rust
let balance_rupiah: i64 = 1_500_000;
```

may be appropriate for a financial model that represents whole Rupiah using integer arithmetic.

The important question is not:

> "Which type is easiest to use?"

The better question is:

> "Does this type correctly represent the domain and its constraints?"

Type selection is therefore part of software design.

---

## Lesson Status

Status: Completed

Completed:

* [x] Understand data types.
* [x] Understand Rust's static type system.
* [x] Understand type inference.
* [x] Understand explicit types.
* [x] Understand scalar types.
* [x] Understand integer types.
* [x] Understand signed and unsigned integers.
* [x] Understand floating-point types.
* [x] Understand `bool` and `char`.
* [x] Understand tuples.
* [x] Understand arrays.
* [x] Understand basic type conversion.
* [x] Understand type mismatch.
* [x] Complete hands-on lab.
* [x] Analyze compiler error.
* [x] Complete code review.

Next lesson:

`Lesson 03 — Expressions and Statements`
