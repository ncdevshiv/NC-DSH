#[cfg(test)]
mod tests;

#[allow(unused_imports)]
pub use moli_selector::{QueryEngine, SelectorError, SelectorErrorKind};

pub mod error {
    #[allow(unused_imports)]
    pub use moli_selector::{SelectorError, SelectorErrorKind};
}
