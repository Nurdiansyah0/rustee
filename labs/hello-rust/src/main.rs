fn calculate_discount(price: i32) -> i32 {
    if price > 50_000 { 10_000 } else { 0 }
}

fn main() {
    let discount = calculate_discount(100_000);

    println!("Discount: {}", discount);
}
