use std::collections::HashMap;
use std::fmt::Display;
use std::fs::File;
use std::io;
use std::io::Error;
#[derive(Debug)]
struct AppError {
    kind: String,    // 错误类型
    message: String, // 错误信息
}
impl From<io::Error> for AppError {
    fn from(error: io::Error) -> Self {
        AppError {
            kind: String::from("io"),
            message: error.to_string(),
        }
    }
}
#[derive(thiserror::Error, Debug)]
enum MyError {
    #[error("Environment variable not found")]
    EnvironmentVariableNotFound(#[from] std::env::VarError),
    #[error(transparent)]
    IOError(#[from] std::io::Error),
}

struct User {
    name: String,
}
impl User {
    fn func(&self) {
        let xx = self; // 报错，解引用报错，self自身不是所有者，例如user.func()时，user才是所有者

        if (*self).name < "hello".to_string() {} // 不报错，比较时会转换为&((*self).name) < &("hello".to_string())
    }
}

#[derive(Clone, Debug)]
struct LargeData {
    data: Vec<u8>,
}
#[tokio::test]
async fn main() -> Result<(), MyError> {
    Ok(())
}
