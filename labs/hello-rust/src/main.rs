fn calculate_total(price: i32, quantity: i32) -> i32 {
    price * quantity
}

fn calculate_tax(total: i32) -> i32 {
    total / 10
}

fn main() {
    let price = 10_000;
    let quantity = 3;

    let total = calculate_total(price, quantity);
    let tax = calculate_tax(total);
    let grand_total = total + tax;

    println!("Price: {}", price);
    println!("Quantity: {}", quantity);
    println!("Total: {}", total);
    println!("Tax: {}", tax);
    println!("Grand Total: {}", grand_total);
}
