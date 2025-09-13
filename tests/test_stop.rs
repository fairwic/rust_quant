#[cfg(test)]
mod tests {
    async fn hello() -> &'static str {
        "Hello, World!"
    }

    #[tokio::test]
    async fn test_hello() {
        let result = hello().await;
        assert_eq!(result, "Hello, World!");
    }
}

