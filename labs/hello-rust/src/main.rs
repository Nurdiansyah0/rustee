fn main() {
    let name = "Nurdiansyah";

    {
        let age = 31;
        println!("{}", name); // OK
        println!("{}", age); // OK
    }

    println!("{}", name); // OK
}
