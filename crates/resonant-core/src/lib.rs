//! Foundational shared functionality for Project Resonant.

/// Product name used throughout the application.
pub const PRODUCT_NAME: &str = "Project Resonant";

/// Current development version.
pub const PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Returns the product name and version.
#[must_use]
pub fn product_identity() -> String {
    format!("{PRODUCT_NAME} v{PRODUCT_VERSION}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn product_identity_contains_name_and_version() {
        let identity = product_identity();

        assert!(identity.contains(PRODUCT_NAME));
        assert!(identity.contains(PRODUCT_VERSION));
    }
}
