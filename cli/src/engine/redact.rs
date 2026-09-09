//! Keeping secrets out of plan output.
//!
//! A diff is the one place hats deliberately prints file contents, and some of
//! those files hold tokens. Two mechanisms, layered, because neither is enough
//! alone:
//!
//! 1. **Value masking.** Every known secret value is replaced wherever it
//!    appears, on both sides of the diff. This catches the current values.
//! 2. **Whole-file withholding.** A file flagged `secret: true` in the manifest
//!    shows only a line count. This catches values the redactor cannot know,
//!    such as a rotated token still sitting in the old file on disk.

use crate::secrets::store::{Secrets, placeholder};

/// Replaces known secret values with `«secret:name»`.
#[derive(Debug, Clone, Default)]
pub struct Redactor {
    /// Longest first, so a secret that contains another is masked whole.
    values: Vec<(String, String)>,
}

impl Redactor {
    pub fn new(secrets: &Secrets) -> Self {
        let mut values = secrets.maskable();
        values.sort_by_key(|(_, v)| std::cmp::Reverse(v.len()));
        Self { values }
    }

    /// A redactor that masks nothing, for `--show-secrets`.
    pub fn disabled() -> Self {
        Self { values: Vec::new() }
    }

    pub fn is_enabled(&self) -> bool {
        !self.values.is_empty()
    }

    /// Mask every known secret value in `text`.
    pub fn mask(&self, text: &str) -> String {
        let mut out = text.to_string();
        for (key, value) in &self.values {
            if out.contains(value.as_str()) {
                out = out.replace(value.as_str(), &placeholder(key));
            }
        }
        out
    }

    /// Mask bytes that may not be UTF-8, leaving non-text content alone.
    pub fn mask_bytes(&self, bytes: &[u8]) -> Vec<u8> {
        match std::str::from_utf8(bytes) {
            Ok(text) => self.mask(text).into_bytes(),
            Err(_) => bytes.to_vec(),
        }
    }

    /// Whether `text` still contains any known secret. Used by a test to prove
    /// masking is total.
    pub fn leaks(&self, text: &str) -> Vec<&str> {
        self.values
            .iter()
            .filter(|(_, v)| text.contains(v.as_str()))
            .map(|(k, _)| k.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn secrets(pairs: &[(&str, &str)]) -> Secrets {
        Secrets::new(
            pairs
                .iter()
                .map(|(k, v)| ((*k).to_string(), secrecy::SecretString::from(*v)))
                .collect::<BTreeMap<_, _>>(),
            None,
        )
    }

    #[test]
    fn a_secret_value_is_replaced_by_its_name() {
        let r = Redactor::new(&secrets(&[("jira_token", "tok-abc123")]));
        assert_eq!(
            r.mask("export JIRA_API_TOKEN=tok-abc123"),
            "export JIRA_API_TOKEN=«secret:jira_token»"
        );
    }

    #[test]
    fn every_occurrence_is_masked_not_just_the_first() {
        let r = Redactor::new(&secrets(&[("k", "sekrit")]));
        let masked = r.mask("sekrit and sekrit again");
        assert!(r.leaks(&masked).is_empty(), "{masked}");
    }

    /// A short value like "ab" would mangle unrelated text, so the store never
    /// offers it for masking.
    #[test]
    fn very_short_values_are_not_masked() {
        let r = Redactor::new(&secrets(&[("tiny", "ab")]));
        assert!(!r.is_enabled());
        assert_eq!(
            r.mask("a table of abbreviations"),
            "a table of abbreviations"
        );
    }

    /// If one secret is a substring of another, masking the shorter one first
    /// would leave a fragment of the longer one visible.
    #[test]
    fn overlapping_secrets_are_masked_longest_first() {
        let r = Redactor::new(&secrets(&[("short", "abcd"), ("long", "abcdefgh")]));
        let masked = r.mask("value=abcdefgh");
        assert_eq!(masked, "value=«secret:long»");
        assert!(r.leaks(&masked).is_empty());
    }

    #[test]
    fn disabled_masks_nothing() {
        let r = Redactor::disabled();
        assert!(!r.is_enabled());
        assert_eq!(r.mask("tok-abc123"), "tok-abc123");
    }

    #[test]
    fn non_utf8_content_passes_through_untouched() {
        let r = Redactor::new(&secrets(&[("k", "sekrit")]));
        let bytes = [0xff, 0xfe, 0x00, 0x01];
        assert_eq!(r.mask_bytes(&bytes), bytes.to_vec());
    }

    #[test]
    fn utf8_bytes_are_masked() {
        let r = Redactor::new(&secrets(&[("k", "sekrit")]));
        let masked = r.mask_bytes(b"a sekrit value");
        assert_eq!(String::from_utf8(masked).unwrap(), "a «secret:k» value");
    }

    /// The property that matters: after masking, no known secret survives
    /// anywhere in the output.
    #[test]
    fn masking_leaves_no_secret_behind() {
        let r = Redactor::new(&secrets(&[
            ("a", "aaaa-1111"),
            ("b", "bbbb-2222"),
            ("c", "cccc-3333"),
        ]));
        let text = "one aaaa-1111, two bbbb-2222,\nthree cccc-3333 and aaaa-1111 again";
        let masked = r.mask(text);
        assert!(
            r.leaks(&masked).is_empty(),
            "leaked: {:?}",
            r.leaks(&masked)
        );
        assert_eq!(masked.matches("«secret:a»").count(), 2);
    }
}
