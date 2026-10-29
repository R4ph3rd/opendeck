//! OpenDeck contracts: the cross-crate types and traits every other crate builds on.
//!
//! This crate sits at the bottom of the dependency graph (ADR 0002). It has no
//! internal dependencies and no required external ones. Any change here is a
//! contract change and goes through the architect.

/// Maximum number of decks the architecture supports. The MVP UI shows two.
pub const MAX_DECKS: usize = 4;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supports_four_decks() {
        assert_eq!(MAX_DECKS, 4);
    }
}
