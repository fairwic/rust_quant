#[tokio::test]
async fn main() {
    let counter = vec![1, 1, 2, 4, 5];
    let a: Vec<i32> = counter
        .iter()
        .enumerate()
        .filter(|&(x, y)| x == (*y as usize))
        .map(|(a, b)| a as i32 * *b)
        .collect();

    println!("count{:?}", a);
    println!("i8max{:?}", i8::MAX);
    println!("count{:?}", counter);
}
