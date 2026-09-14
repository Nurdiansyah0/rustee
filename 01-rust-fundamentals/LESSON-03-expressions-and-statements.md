# Lesson 03 — Expressions and Statements

## Learning Objectives

By the end of this lesson, I should understand:

* What an expression is in Rust.
* What a statement is in Rust.
* The difference between expressions and statements.
* Why expressions produce values.
* How statements perform actions without directly producing values.
* How assignments behave in Rust.
* How blocks can be expressions.
* How the final expression of a block becomes its return value.
* The difference between an expression with and without a semicolon.
* How expressions are used with `let`.
* How functions can return values through expressions.
* How Rust's expression-oriented design affects program structure.
* How the compiler detects incorrect expression and statement usage.
* Why understanding expressions and statements is important for writing idiomatic Rust.

---

## 1. What Is an Expression?

An expression is code that evaluates to a value.

For example:

```rust
5
```

is an expression because it evaluates to the value `5`.

An arithmetic operation is also an expression:

```rust
5 + 3
```

The expression evaluates to:

```text
8
```

A variable can also be used as an expression:

```rust
let age = 31;

age
```

The expression `age` evaluates to the value stored in `age`.

Expressions are fundamental to Rust because many constructs produce values.

---

## 2. What Is a Statement?

A statement performs an action.

For example:

```rust
let age = 31;
```

is a statement.

The statement creates a binding named `age`.

Another example:

```rust
println!("Hello");
```

is an expression used as a statement.

The important distinction is:

```text
Expression → evaluates to a value

Statement → performs an action
```

Rust uses both expressions and statements to construct programs.

---

## 3. Expressions vs Statements

Consider:

```rust
let age = 31;
```

The complete `let` declaration is a statement.

The value:

```rust
31
```

is an expression.

Another example:

```rust
let result = 10 + 20;
```

Here:

```text
10 + 20
```

is an expression.

It evaluates to:

```text
30
```

The `let` declaration uses that resulting value to initialize `result`.

Therefore:

```rust
let result = 10 + 20;
```

can be conceptually understood as:

```text
statement
    └── expression
          └── produces 30
```

Understanding this distinction becomes important when working with functions, blocks, conditionals, and control flow.

---

## 4. The Semicolon

One of the most important details in Rust is the semicolon.

Compare:

```rust
5 + 3
```

with:

```rust
5 + 3;
```

The first is an expression that produces:

```text
8
```

The second is an expression followed by a semicolon and used as a statement.

The semicolon changes how the expression is used.

This distinction becomes especially important when returning values from blocks and functions.

For example:

```rust
let result = {
    5 + 3
};
```

The block evaluates to:

```text
8
```

But:

```rust
let result = {
    5 + 3;
};
```

does not produce the same value.

The semicolon causes the expression to be treated as a statement.

---

## 5. Blocks Are Expressions

Rust blocks can themselves produce values.

For example:

```rust
let result = {
    let x = 10;
    let y = 20;

    x + y
};
```

The block contains several statements:

```rust
let x = 10;
let y = 20;
```

and a final expression:

```rust
x + y
```

The value of the final expression becomes the value of the entire block.

Therefore:

```rust
result
```

contains:

```text
30
```

This is an important characteristic of Rust's expression-oriented design.

---

## 6. Final Expression of a Block

The final expression determines the value returned by a block.

Example:

```rust
let result = {
    let x = 10;
    let y = 20;

    x + y
};
```

The final expression is:

```rust
x + y
```

Therefore the block produces:

```text
30
```

The following is different:

```rust
let result = {
    let x = 10;
    let y = 20;

    x + y;
};
```

Because of the semicolon, the final expression is no longer producing the value of the block in the same way.

The resulting value becomes the unit type:

```text
()
```

This distinction is fundamental when writing functions and control-flow expressions.

---

## 7. The Unit Type

Rust has a special type called the unit type:

```text
()
```

The unit type represents the absence of a meaningful value.

For example:

```rust
println!("Hello");
```

does not produce a useful application value.

Its return value is:

```text
()
```

A function that does not return a meaningful value can therefore have the type:

```rust
fn log_message() -> () {
    println!("Hello");
}
```

The `-> ()` can normally be omitted:

```rust
fn log_message() {
    println!("Hello");
}
```

This is commonly used for functions whose purpose is performing an action rather than returning data.

---

## 8. Expressions in `let` Bindings

The right-hand side of a `let` binding is an expression.

For example:

```rust
let age = 30 + 1;
```

The expression:

```rust
30 + 1
```

is evaluated first.

The result:

```text
31
```

is then assigned to `age`.

Expressions can also become more complex:

```rust
let result = {
    let x = 10;
    let y = 20;

    x + y
};
```

The block is an expression, so it can be used as the value assigned to `result`.

---

## 9. Function Return Values

Rust functions can return values.

For example:

```rust
fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

The return type is:

```text
i32
```

The final expression:

```rust
a + b
```

becomes the return value.

The function can then be used as an expression:

```rust
let result = add(10, 20);
```

The function call evaluates to:

```text
30
```

The semicolon matters.

This:

```rust
fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

returns the result.

While:

```rust
fn add(a: i32, b: i32) -> i32 {
    a + b;
}
```

does not return `i32` as required.

The compiler will detect the mismatch.

---

## 10. Expression-Oriented Rust

Rust is an expression-oriented language.

Many constructs produce values.

For example, a block:

```rust
let value = {
    10 + 20
};
```

produces a value.

Conditional expressions can also produce values:

```rust
let result = if true {
    10
} else {
    20
};
```

The `if` expression evaluates to one of the branch values.

This allows code to be composed from smaller expressions.

Instead of thinking only in terms of:

```text
perform action
perform action
perform action
```

Rust encourages reasoning about:

```text
expression
    ↓
value
    ↓
next expression
```

This model becomes increasingly important when working with functions, ownership, error handling, iterators, and pattern matching.

---

## 11. Statements Inside Expressions

A block can contain statements and finish with an expression.

Example:

```rust
let result = {
    let first = 10;
    let second = 20;

    first + second
};
```

The structure is:

```text
block
├── statement
├── statement
└── final expression
```

The statements prepare the data.

The final expression produces the value of the block.

This pattern appears frequently in idiomatic Rust.

---

## 12. Deliberate Compiler Failure

We will deliberately create a type mismatch involving a function return value.

Start with:

```rust
fn add(a: i32, b: i32) -> i32 {
    a + b;
}
```

Then:

```rust
fn main() {
    let result = add(10, 20);

    println!("result: {}", result);
}
```

Run:

```bash
cargo check --manifest-path labs/hello-rust/Cargo.toml
```

The compiler should report an error because the function promises:

```text
i32
```

but the block does not return the required `i32` value.

The semicolon changed:

```rust
a + b
```

from the final value-producing expression into a statement.

The compiler therefore detects the mismatch between the declared return type and the actual block value.

---

## 13. Correct Implementation

Remove the semicolon:

```rust
fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

Then:

```rust
fn main() {
    let result = add(10, 20);

    println!("result: {}", result);
}
```

Run:

```bash
cargo fmt --manifest-path labs/hello-rust/Cargo.toml -- --check
```

Then:

```bash
cargo check --manifest-path labs/hello-rust/Cargo.toml
```

Then:

```bash
cargo run --manifest-path labs/hello-rust/Cargo.toml
```

Expected output:

```text
result: 30
```

Finally:

```bash
cargo clippy --manifest-path labs/hello-rust/Cargo.toml -- -D warnings
```

The implementation should pass all checks.

---

## 14. Hands-on Lab

Modify:

```text
labs/hello-rust/src/main.rs
```

to implement the following concepts.

First, create a simple arithmetic function:

```rust
fn calculate_total(price: i32, quantity: i32) -> i32 {
    price * quantity
}
```

Then use it from `main`:

```rust
fn main() {
    let price = 10_000;
    let quantity = 3;

    let total = calculate_total(price, quantity);

    println!("price: {}", price);
    println!("quantity: {}", quantity);
    println!("total: {}", total);
}
```

Then add a block expression:

```rust
let subtotal = {
    let price = 10_000;
    let quantity = 3;

    price * quantity
};
```

Print the result:

```rust
println!("subtotal: {}", subtotal);
```

Then create an expression using an existing value:

```rust
let tax = subtotal / 10;
```

Calculate the final value:

```rust
let grand_total = subtotal + tax;
```

Print:

```rust
println!("tax: {}", tax);
println!("grand total: {}", grand_total);
```

The final program should demonstrate:

* Function expressions.
* Arithmetic expressions.
* `let` statements.
* Block expressions.
* Final expressions.
* Function return values.
* Expressions used to initialize variables.

Verify the program:

```bash
cargo fmt --manifest-path labs/hello-rust/Cargo.toml -- --check
```

```bash
cargo check --manifest-path labs/hello-rust/Cargo.toml
```

```bash
cargo run --manifest-path labs/hello-rust/Cargo.toml
```

```bash
cargo clippy --manifest-path labs/hello-rust/Cargo.toml -- -D warnings
```

---

## 15. Engineering Takeaways

Expressions and statements are not merely syntax rules.

They determine how values flow through a Rust program.

A useful mental model is:

```text
Statement
    ↓
prepares or performs an action

Expression
    ↓
produces a value
```

For blocks:

```text
{
    statements...

    final expression
}
        ↓
     value
```

For functions:

```text
function call
      ↓
   expression
      ↓
 return value
```

The semicolon is therefore more than formatting.

Compare:

```rust
value
```

with:

```rust
value;
```

The first can be the value of the surrounding expression or block.

The second turns it into a statement and results in the unit value when it is the final construct of a block.

Understanding this behavior prevents subtle mistakes in function return values and nested expressions.

---

## 16. Senior Engineering Perspective

At a senior engineering level, expressions and statements should be understood as part of Rust's program composition model.

When reviewing Rust code, ask:

* What value does this expression produce?
* Is this construct being used as a statement or an expression?
* Does the final expression of this block have the intended type?
* Is a semicolon intentionally present?
* Is a semicolon accidentally discarding a value?
* Can this block be simplified without reducing clarity?
* Does the function's return type match the value produced by its body?
* Does the expression make the data flow easier or harder to understand?

For example:

```rust
let total = {
    let price = 10_000;
    let quantity = 3;

    price * quantity
};
```

The block expresses a small computation.

Its intermediate bindings provide context:

```text
price
quantity
   ↓
multiplication
   ↓
total
```

This can be clearer than unnecessarily introducing mutable state:

```rust
let mut total = 0;
total = 10_000 * 3;
```

Expression-oriented design can therefore reduce mutable state and make data flow more explicit.

However, expression-oriented code should not be used merely to make code shorter.

Senior-level code review should prioritize:

```text
Correctness
    ↓
Clarity
    ↓
Maintainability
    ↓
Composability
```

rather than minimizing line count.

---

## Lesson Status

Status: Completed

Completed:

* [x] Understand expressions.
* [x] Understand statements.
* [x] Understand the difference between expressions and statements.
* [x] Understand semicolon behavior.
* [x] Understand block expressions.
* [x] Understand final expressions.
* [x] Understand the unit type.
* [x] Understand expressions in `let` bindings.
* [x] Understand function return values.
* [x] Understand expression-oriented Rust.
* [x] Complete hands-on lab.
* [x] Analyze compiler error.
* [x] Complete code review.

Next lesson:

`Lesson 04 — Functions`
