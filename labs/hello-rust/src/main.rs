fn main() {
    let age = 31;
    let temperature = 36.5;
    let active = true;
    let initial = 'N';

    let integer: i32 = 31;
    let large_integer: i64 = 31;
    let converted = i64::from(integer);

    let user = ("Nurdiansyah", age, active);
    let numbers: [i32; 4] = [10, 20, 30, 40];

    println!("age: {}", age);
    println!("temperature: {}", temperature);
    println!("active: {}", active);
    println!("initial: {}", initial);

    println!("integer: {}", integer);
    println!("large integer: {}", large_integer);
    println!("converted: {}", converted);

    println!("name: {}", user.0);
    println!("user age: {}", user.1);
    println!("user active: {}", user.2);

    println!("first number: {}", numbers[0]);
}
