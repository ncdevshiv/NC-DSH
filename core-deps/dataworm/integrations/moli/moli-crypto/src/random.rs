use aws_lc_rs::rand::{SecureRandom, SystemRandom};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SecureRandomError;

impl std::fmt::Display for SecureRandomError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("secure random source unavailable")
    }
}

impl std::error::Error for SecureRandomError {}

pub fn fill_secure_random(output: &mut [u8]) -> Result<(), SecureRandomError> {
    SystemRandom::new()
        .fill(output)
        .map_err(|_| SecureRandomError)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secure_random_accepts_empty_and_nonempty_outputs() {
        fill_secure_random(&mut []).expect("empty secure-random fill should succeed");
        fill_secure_random(&mut [0_u8; 32]).expect("secure-random fill should succeed");
    }
}
