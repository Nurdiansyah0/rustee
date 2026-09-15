# Lesson 04 — Functions

## Learning Objectives

By the end of this lesson, I should understand:

* What a function is in Rust.
* How to define a function.
* How to call a function.
* How function names are written in Rust.
* How parameters work.
* How parameter types are declared.
* How arguments are passed to functions.
* The difference between parameters and arguments.
* How functions can return values.
* How return types are declared.
* How the final expression of a function becomes its return value.
* The difference between implicit returns and explicit `return`.
* How functions interact with Rust's type system.
* How functions create scope.
* How values move into and out of functions.
* How functions improve code decomposition.
* How the compiler detects incorrect function calls.
* How to test and review functions.
* Why function design is important for maintainable Rust systems.

---

## 1. What Is a Function?

A function is a named unit of behavior.

Functions allow a program to group instructions into a reusable piece of code.

For example:

```rust
fn greet() {
    println!("Hello, Rust!");
}
```

The function is named:

```text
greet
```

The function contains:

```rust
println!("Hello, Rust!");
```

However, defining a function does not execute it.

The function must be called:

```rust
fn greet() {
    println!("Hello, Rust!");
}

fn main() {
    greet();
}
```

The execution flow is:

```text
main()
  ↓
greet()
  ↓
println!
```

Functions therefore provide a way to structure program behavior into named operations.

---

## 2. Function Declaration

Rust uses the `fn` keyword to define a function.

Basic syntax:

```rust
fn function_name() {
    // function body
}
```

Example:

```rust
fn say_hello() {
    println!("Hello");
}
```

The components are:

```text
fn
│
├── function name
│
├── parameter list
│
└── function body
```

The parameter list can be empty:

```rust
fn say_hello() {
    println!("Hello");
}
```

Or contain parameters:

```rust
fn greet(name: &str) {
    println!("Hello, {}", name);
}
```

The function body is a block:

```rust
{
    println!("Hello, {}", name);
}
```

As learned in Lesson 03, blocks are expressions in Rust.

This becomes important when functions return values.

---

## 3. Calling a Function

A function is called by writing its name followed by parentheses:

```rust
greet();
```

Example:

```rust
fn greet() {
    println!("Hello");
}

fn main() {
    greet();
    greet();
}
```

The function can be called multiple times.

The execution becomes:

```text
main
 │
 ├── greet
 │
 └── greet
```

This allows behavior to be reused without duplicating the implementation.

Instead of:

```rust
println!("Hello");
println!("Hello");
println!("Hello");
```

we can write:

```rust
fn greet() {
    println!("Hello");
}

fn main() {
    greet();
    greet();
    greet();
}
```

The behavior is defined once and reused.

---

## 4. Function Naming

Rust convention uses `snake_case` for function names.

Examples:

```rust
fn calculate_total() {}
fn calculate_tax() {}
fn process_payment() {}
fn load_configuration() {}
```

Avoid names such as:

```rust
fn CalculateTotal() {}
fn calculateTotal() {}
```

The conventional Rust style is:

```text
snake_case
```

This is not merely cosmetic.

Following established naming conventions improves consistency across Rust codebases and makes code easier to review.

---

## 5. Parameters

A function can receive input through parameters.

Example:

```rust
fn greet(name: &str) {
    println!("Hello, {}", name);
}
```

Here:

```text
name
```

is the parameter.

Its type is:

```text
&str
```

The syntax is:

```rust
parameter_name: Type
```

Multiple parameters are separated by commas:

```rust
fn add(a: i32, b: i32) {
    println!("{}", a + b);
}
```

Here there are two parameters:

```text
a → i32
b → i32
```

Rust requires parameter types to be explicitly declared.

For example:

```rust
fn add(a: i32, b: i32) {
    println!("{}", a + b);
}
```

This is different from local variable type inference:

```rust
let a = 10;
```

The compiler can infer the type of a local variable.

Function parameter types form part of the function's interface and therefore must be declared.

---

## 6. Arguments

An argument is the actual value passed to a function when the function is called.

Example:

```rust
fn greet(name: &str) {
    println!("Hello, {}", name);
}

fn main() {
    greet("Nurdiansyah");
}
```

Here:

```text
name
```

is the parameter.

And:

```text
"Nurdiansyah"
```

is the argument.

The relationship is:

```text
Parameter:
name: &str

Argument:
"Nurdiansyah"
```

Another example:

```rust
fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn main() {
    let result = add(10, 20);
}
```

The parameters are:

```text
a
b
```

The arguments are:

```text
10
20
```

A useful distinction is:

```text
Parameter → variable defined by the function

Argument → value supplied by the caller
```

---

## 7. Function Return Types

A function can return a value.

The return type is specified using:

```text
-> Type
```

Example:

```rust
fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

The function promises to return:

```text
i32
```

The syntax is:

```text
fn function_name(parameters) -> return_type
```

For example:

```rust
fn square(number: i32) -> i32 {
    number * number
}
```

The return type is:

```text
i32
```

Calling the function produces a value:

```rust
let result = square(5);
```

The resulting value is:

```text
25
```

A function call is therefore an expression.

Conceptually:

```text
square(5)
    ↓
expression
    ↓
25
```

---

## 8. Implicit Return

Rust allows a function to return the final expression of its body without using `return`.

Example:

```rust
fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

The final expression is:

```rust
a + b
```

Because there is no semicolon, its value becomes the function's return value.

This follows the same block-expression behavior studied in Lesson 03.

Conceptually:

```text
function body
{
    a + b
}
      ↓
    return value
```

For:

```rust
add(10, 20)
```

the result is:

```text
30
```

This is the idiomatic Rust style for simple return logic.

---

## 9. Explicit `return`

Rust also supports an explicit `return` statement.

Example:

```rust
fn add(a: i32, b: i32) -> i32 {
    return a + b;
}
```

This is valid Rust.

The function immediately returns the value:

```text
a + b
```

The difference is:

Implicit return:

```rust
fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

Explicit return:

```rust
fn add(a: i32, b: i32) -> i32 {
    return a + b;
}
```

Both produce the same result.

Idiomatic Rust generally prefers the implicit final expression for straightforward return logic.

Explicit `return` is useful when an early exit improves clarity.

For example:

```rust
fn divide(a: i32, b: i32) -> i32 {
    if b == 0 {
        return 0;
    }

    a / b
}
```

The explicit `return` communicates an early exit.

---

## 10. The Semicolon and Function Returns

The semicolon remains important inside function bodies.

Compare:

```rust
fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

with:

```rust
fn add(a: i32, b: i32) -> i32 {
    a + b;
}
```

The first returns the value of:

```rust
a + b
```

The second turns the expression into an expression statement.

The resulting block value becomes:

```text
()
```

But the function promises:

```text
i32
```

Therefore the compiler reports a type mismatch.

This is one of the most common beginner mistakes when learning Rust functions.

The mental model should be:

```text
final expression without ;
        ↓
function return value

final expression with ;
        ↓
expression statement
        ↓
()
```

---

## 11. Functions Without Return Values

A function does not need to return a meaningful application value.

Example:

```rust
fn log_message(message: &str) {
    println!("{}", message);
}
```

The function performs an action.

Its return type is effectively:

```text
()
```

The following is equivalent:

```rust
fn log_message(message: &str) -> () {
    println!("{}", message);
}
```

However, explicitly writing `-> ()` is normally unnecessary.

The idiomatic form is:

```rust
fn log_message(message: &str) {
    println!("{}", message);
}
```

This type of function is useful for operations such as:

```text
logging
printing
mutating external state
performing an action
```

---

## 12. Function Scope

Function parameters and local variables belong to the function's scope.

Example:

```rust
fn calculate() {
    let value = 100;

    println!("{}", value);
}

fn main() {
    calculate();
}
```

The variable:

```text
value
```

exists inside `calculate`.

It cannot automatically be accessed from `main`.

For example:

```rust
fn calculate() {
    let value = 100;
}

fn main() {
    calculate();

    println!("{}", value);
}
```

This will fail because `value` is not in scope inside `main`.

The function creates a boundary for local variables.

Conceptually:

```text
main scope
│
├── calculate()
│   └── value
│
└── cannot directly access value
```

This separation is important for modular program design.

---

## 13. Passing Values Into Functions

Values can be passed into functions through arguments.

Example:

```rust
fn square(number: i32) -> i32 {
    number * number
}

fn main() {
    let value = 5;

    let result = square(value);

    println!("{}", result);
}
```

The value flows:

```text
value
  ↓
square(value)
  ↓
number
  ↓
number * number
  ↓
25
  ↓
result
```

This makes the data flow explicit.

Functions therefore allow us to transform inputs into outputs.

A useful abstraction is:

```text
input
  ↓
function
  ↓
output
```

For example:

```text
price + quantity
       ↓
calculate_total
       ↓
total
```

---

## 14. Multiple Parameters

Functions can accept multiple parameters.

Example:

```rust
fn calculate_total(price: i32, quantity: i32) -> i32 {
    price * quantity
}
```

Calling:

```rust
let total = calculate_total(10_000, 3);
```

produces:

```text
30_000
```

The parameter mapping is:

```text
price    ← 10_000
quantity ← 3
```

The function then evaluates:

```text
10_000 * 3
```

Result:

```text
30_000
```

The order of arguments matters.

For example:

```rust
calculate_total(10_000, 3);
```

is not conceptually equivalent to:

```rust
calculate_total(3, 10_000);
```

unless the function's logic happens to make those values interchangeable.

---

## 15. Function Type Contract

A function signature describes its interface.

Consider:

```rust
fn calculate_total(price: i32, quantity: i32) -> i32 {
    price * quantity
}
```

The signature communicates:

```text
Name:
calculate_total

Inputs:
price: i32
quantity: i32

Output:
i32
```

This can be viewed as a contract:

```text
(i32, i32) → i32
```

The caller knows what the function expects and what it produces.

This is an important software engineering principle.

A good function interface should make its contract clear.

---

## 16. Deliberate Compiler Failure — Wrong Argument Type

We will deliberately pass an incorrect type.

Start with:

```rust
fn square(number: i32) -> i32 {
    number * number
}
```

Then:

```rust
fn main() {
    let value = "5";

    let result = square(value);

    println!("{}", result);
}
```

Run:

```bash
cargo check --manifest-path labs/hello-rust/Cargo.toml
```

The compiler should reject the program.

The function expects:

```text
i32
```

but the argument is:

```text
&str
```

Conceptually:

```text
Expected:
i32

Received:
&str
```

Rust's static type system catches the problem before the program runs.

---

## 17. Correcting the Type Error

Change the argument to an integer:

```rust
fn main() {
    let value = 5;

    let result = square(value);

    println!("{}", result);
}
```

Now the types match:

```text
value
  ↓
i32
  ↓
square(i32)
  ↓
i32
```

Run:

```bash
cargo check --manifest-path labs/hello-rust/Cargo.toml
```

Then:

```bash
cargo run --manifest-path labs/hello-rust/Cargo.toml
```

Expected output:

```text
25
```

---

## 18. Deliberate Compiler Failure — Wrong Number of Arguments

A function's parameter count is also part of its contract.

Given:

```rust
fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

The function requires two arguments.

This is valid:

```rust
add(10, 20);
```

But this is invalid:

```rust
add(10);
```

And this is also invalid:

```rust
add(10, 20, 30);
```

The compiler will detect the incorrect number of arguments.

This demonstrates that Rust validates function calls against their signatures.

---

## 19. Function Composition

Functions can be combined.

Example:

```rust
fn add(a: i32, b: i32) -> i32 {
    a + b
}

fn double(value: i32) -> i32 {
    value * 2
}

fn main() {
    let result = double(add(10, 20));

    println!("{}", result);
}
```

The execution can be understood as:

```text
add(10, 20)
      ↓
     30
      ↓
double(30)
      ↓
     60
```

Each function performs a specific transformation.

This is an early example of composability.

Instead of creating one large function, behavior can be divided into smaller operations.

---

## 20. Functions and Data Flow

Consider:

```rust
fn calculate_total(price: i32, quantity: i32) -> i32 {
    price * quantity
}

fn calculate_tax(total: i32) -> i32 {
    total / 10
}
```

Then:

```rust
fn main() {
    let total = calculate_total(10_000, 3);
    let tax = calculate_tax(total);

    println!("total: {}", total);
    println!("tax: {}", tax);
}
```

The data flow is:

```text
price + quantity
       ↓
calculate_total
       ↓
total
       ↓
calculate_tax
       ↓
tax
```

Each function has a focused responsibility.

This becomes increasingly important as applications become larger.

---

## 21. Functions and Expressions

A function call is an expression.

For example:

```rust
let total = calculate_total(10_000, 3);
```

The function call:

```rust
calculate_total(10_000, 3)
```

produces a value.

That value is used by the `let` statement.

Conceptually:

```text
let statement
    │
    └── function-call expression
              │
              └── produces value
```

This connects Lesson 03 directly with functions.

Rust's expression-oriented design allows function calls to participate naturally in larger expressions.

For example:

```rust
let grand_total = calculate_total(price, quantity) + tax;
```

The function call produces a value that becomes part of another expression.

---

## 22. Functions and Block Expressions

A function body is a block.

For example:

```rust
fn calculate_total(price: i32, quantity: i32) -> i32 {
    let subtotal = price * quantity;

    subtotal
}
```

The function body contains:

```text
statement:
let subtotal = price * quantity;

final expression:
subtotal
```

Therefore:

```text
subtotal
   ↓
function return value
```

The structure is:

```text
function
│
└── block
    ├── statements
    └── final expression
             ↓
        return value
```

This is one of the most important connections between Lesson 03 and Lesson 04.

---

## 23. Early Return

Explicit `return` is useful when a function must terminate before reaching its final expression.

Example:

```rust
fn absolute_difference(a: i32, b: i32) -> i32 {
    if a >= b {
        return a - b;
    }

    b - a
}
```

The first branch exits immediately.

If:

```text
a >= b
```

the function returns:

```text
a - b
```

Otherwise the function reaches:

```rust
b - a
```

The final expression handles the normal path.

This can be useful when early validation or guard conditions are required.

---

## 24. Functions and Validation

Functions can encapsulate validation logic.

For example:

```rust
fn calculate_discount(price: i32) -> i32 {
    if price > 50_000 {
        10_000
    } else {
        0
    }
}
```

The function receives:

```text
price
```

and returns:

```text
discount
```

The interface is:

```text
i32 → i32
```

The caller does not need to know the internal implementation.

It only needs to know:

```text
input:
price

output:
discount
```

This separation allows implementation details to change without requiring every caller to change.

---

## 25. Function Responsibility

A function should generally have a clear responsibility.

Consider:

```rust
fn calculate_total(price: i32, quantity: i32) -> i32 {
    price * quantity
}
```

Its responsibility is clear:

```text
calculate total
```

A less focused function might do many unrelated things:

```text
calculate total
validate user
write database
send email
print report
```

Large functions become harder to:

```text
test
debug
review
reuse
maintain
```

Function decomposition helps separate responsibilities.

A useful engineering question is:

> What single responsibility does this function represent?

This does not mean every function must contain only one line.

It means the function's purpose should be coherent.

---

## 26. Function Size

Small functions are not automatically better.

For example:

```rust
fn multiply(a: i32, b: i32) -> i32 {
    a * b
}
```

is simple and clear.

But unnecessary fragmentation can reduce readability:

```rust
fn get_a() -> i32 {
    10
}

fn get_b() -> i32 {
    20
}

fn add_values(a: i32, b: i32) -> i32 {
    a + b
}
```

If these functions provide no meaningful abstraction, they may make the program harder to understand.

Good function design balances:

```text
cohesion
clarity
reuse
testability
complexity
```

The goal is not simply to create as many functions as possible.

---

## 27. Function Purity

Some functions depend only on their inputs.

For example:

```rust
fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

Given the same inputs, the function produces the same output.

This is often described as a pure function.

Pure functions are generally easier to:

```text
test
reason about
compose
debug
```

Other functions perform side effects.

For example:

```rust
fn log_message(message: &str) {
    println!("{}", message);
}
```

This function interacts with the outside world through standard output.

Later lessons will examine ownership, borrowing, I/O, error handling, and side effects in much greater depth.

---

## 28. Compiler-Driven Development

Rust's compiler can be used as part of the development process.

When writing a function, verify:

```text
function signature
        ↓
parameter types
        ↓
argument types
        ↓
return type
        ↓
function body
```

For example:

```rust
fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

The compiler verifies that:

```text
a → i32
b → i32
a + b → i32
function result → i32
```

If one part does not match, compilation fails.

This makes the function signature a useful form of executable documentation.

---

## 29. Hands-on Lab

Modify:

```text
labs/hello-rust/src/main.rs
```

to implement a small calculation system.

First, create a function:

```rust
fn calculate_total(price: i32, quantity: i32) -> i32 {
    price * quantity
}
```

Then create a tax function:

```rust
fn calculate_tax(total: i32) -> i32 {
    total / 10
}
```

Then use both functions from `main`.

The program should:

1. Define a price.
2. Define a quantity.
3. Calculate the total.
4. Calculate the tax.
5. Calculate the grand total.
6. Print all results.

The intended data flow is:

```text
price + quantity
       ↓
calculate_total
       ↓
subtotal
       ↓
calculate_tax
       ↓
tax
       ↓
subtotal + tax
       ↓
grand_total
```

The implementation should use:

* Function definitions.
* Function parameters.
* Function arguments.
* Explicit parameter types.
* Explicit return types.
* Function calls.
* Implicit return through final expressions.
* `let` bindings.
* Arithmetic expressions.

Do not use mutable variables unless there is a specific reason to do so.

---

## 30. Hands-on Compiler Experiment

After implementing the working version, deliberately introduce an error.

Change:

```rust
fn calculate_total(price: i32, quantity: i32) -> i32 {
    price * quantity
}
```

to:

```rust
fn calculate_total(price: i32, quantity: i32) -> i32 {
    price * quantity;
}
```

Run:

```bash
cargo check --manifest-path labs/hello-rust/Cargo.toml
```

Observe the compiler error.

Then restore:

```rust
fn calculate_total(price: i32, quantity: i32) -> i32 {
    price * quantity
}
```

Run the verification commands again.

---

## 31. Verification

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

Then:

```bash
cargo clippy --manifest-path labs/hello-rust/Cargo.toml -- -D warnings
```

Finally:

```bash
git diff --check
```

All checks should pass before the lesson is considered complete.

---

## 32. Code Review Checklist

Before marking this lesson complete, review the implementation.

Check:

* [x] Function names use `snake_case`.
* [x] Parameters have explicit types.
* [x] Return types are correct.
* [x] Arguments match parameter types.
* [x] The number of arguments is correct.
* [x] Function calls produce the expected values.
* [x] Final expressions are used correctly.
* [x] Accidental semicolons do not discard return values.
* [x] Functions have clear responsibilities.
* [x] Unnecessary mutable state is avoided.
* [x] The code passes `cargo fmt`.
* [x] The code passes `cargo check`.
* [x] The code passes `cargo clippy -D warnings`.
* [x] There are no whitespace errors detected by `git diff --check`.

---

## 33. Engineering Takeaways

Functions provide a fundamental mechanism for decomposition.

A function can be viewed as:

```text
input
  ↓
function
  ↓
output
```

For example:

```text
price + quantity
       ↓
calculate_total
       ↓
total
```

A function signature describes its contract:

```rust
fn calculate_total(price: i32, quantity: i32) -> i32
```

which can be understood as:

```text
(i32, i32) → i32
```

The function receives two integers and produces one integer.

Rust's expression-oriented design means the function body can return its final expression:

```rust
fn add(a: i32, b: i32) -> i32 {
    a + b
}
```

The absence of a semicolon is significant.

The function body is a block expression, and the final expression becomes the block's value.

Functions therefore connect several concepts learned so far:

```text
Variables
   ↓
Data Types
   ↓
Expressions
   ↓
Blocks
   ↓
Functions
   ↓
Return Values
```

---

## 34. Senior Engineering Perspective

At a senior engineering level, functions should be evaluated beyond whether they compile.

When reviewing a function, ask:

* Is the function's responsibility clear?
* Is the function interface understandable?
* Are parameter types appropriate?
* Is the return type meaningful?
* Are side effects obvious?
* Does the function hide unnecessary implementation details?
* Is the function easy to test?
* Is the function reusable where appropriate?
* Does the function make the data flow easier to understand?
* Does the function introduce unnecessary coupling?
* Is the abstraction justified?

For example:

```rust
fn calculate_total(price: i32, quantity: i32) -> i32 {
    price * quantity
}
```

has a simple and explicit contract.

Its behavior is deterministic:

```text
same input
    ↓
same output
```

This makes it easy to test.

A more complex function might interact with:

```text
database
network
filesystem
environment variables
external APIs
global state
```

Those dependencies increase complexity.

Senior engineering therefore considers not only:

```text
Does the function work?
```

but also:

```text
What does the function depend on?

What does the function change?

What guarantees does the function provide?

How can the function fail?

How can the function be tested?

How will the function behave under production load?
```

These questions become increasingly important as we progress toward backend engineering, systems programming, concurrency, error handling, and system design.

---

## Lesson Status

Status: Completed

Completed:

* [x] Understand what a function is.
* [x] Understand function declarations.
* [x] Understand function calls.
* [x] Understand Rust function naming conventions.
* [x] Understand parameters.
* [x] Understand arguments.
* [x] Understand parameter types.
* [x] Understand return types.
* [x] Understand implicit returns.
* [x] Understand explicit `return`.
* [x] Understand semicolon behavior in function returns.
* [x] Understand functions without meaningful return values.
* [x] Understand function scope.
* [x] Understand multiple parameters.
* [x] Understand function contracts.
* [x] Understand function composition.
* [x] Understand functions as expressions.
* [x] Understand functions and block expressions.
* [x] Understand early returns.
* [x] Understand function responsibility.
* [x] Complete hands-on lab.
* [x] Analyze compiler errors.
* [x] Complete code review.
* [x] Complete verification.

Next lesson:

`Lesson 05 — Control Flow`
