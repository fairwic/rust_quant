#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn test_hello() {
        let result = hello().await;
        assert_eq!(result, "Hello, World!");
    }
}

async fn hello() -> &'static str {
    "Hello, World!"
}
