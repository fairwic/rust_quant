use sha2::{Digest, Sha256};
/// 提供sha256的集中实现，避免量化核心调用方重复处理相同细节。
pub fn sha256(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    hex::encode(result)
}
