pub mod bash;
pub mod edit;
pub mod grep;
pub mod read;
pub mod tree;
pub mod write;

use std::hash::{DefaultHasher, Hasher as _};

pub fn hash_line(content: &str) -> String {
    let mut hasher = DefaultHasher::new();
    hasher.write(content.trim_end().as_bytes());
    let hash = hasher.finish();

    // base-36 encodes each digit as one of 10 digits + 26 lowercase letters (0-9, a-z)
    const RADIX: u32 = 36;
    // Extract two independent base-36 digits from different byte regions of the hash.
    // The low byte (hash % RADIX) and the second byte ((hash >> 8) % RADIX) are used so
    // that the two characters vary somewhat independently rather than both deriving
    // from the same narrow range of bits.
    let c1 = char::from_digit((hash % RADIX as u64) as u32, RADIX).unwrap();
    let c2 = char::from_digit(((hash >> 8) % RADIX as u64) as u32, RADIX).unwrap();
    format!("{c1}{c2}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_stable() {
        assert_eq!(hash_line("fn main() {"), hash_line("fn main() {"));
    }

    #[test]
    fn hash_right_trims() {
        assert_eq!(hash_line("hello"), hash_line("hello   "));
        assert_eq!(hash_line("hello"), hash_line("hello\t"));
    }

    #[test]
    fn hash_empty_string() {
        let h = hash_line("");
        assert_eq!(h.len(), 2);
        assert!(h.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn hash_output_is_two_alphanum_chars() {
        for s in ["fn main() {", "    let x = 1;", "", "}", "// comment"] {
            let h = hash_line(s);
            assert_eq!(h.len(), 2, "hash of {s:?} has wrong length");
            assert!(
                h.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()),
                "hash of {s:?} contains non-alphanum char: {h}"
            );
        }
    }

    #[test]
    fn different_content_produces_different_hashes() {
        assert_ne!(hash_line("fn main() {"), hash_line("fn other() {"));
        assert_ne!(hash_line("let x = 1;"), hash_line("let x = 2;"));
    }
}
