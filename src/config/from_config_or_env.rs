//! A secret is either written in the config file or named there and kept in
//! the environment.
//!
//! Two of them are - the stats password and the ipinfo token - and both are
//! read the same way, because the surprising cases are the same for both: a
//! key present but empty means unset, and a value with a stray newline from
//! however it was exported is the value without it.

pub(super) fn secret(configured: Option<&str>, env_var: &str) -> Option<String> {
    configured
        .map(str::trim)
        .filter(|secret| !secret.is_empty())
        .map(str::to_owned)
        .or_else(|| {
            std::env::var(env_var)
                .ok()
                .map(|secret| secret.trim().to_owned())
                .filter(|secret| !secret.is_empty())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_is_written_wins_and_an_empty_setting_is_no_setting() {
        let env_var = "VALIDATORCLOCK_TEST_SECRET_SOURCE";
        // SAFETY: single-threaded test, and the variable is its own.
        unsafe { std::env::set_var(env_var, " from-the-environment\n") };

        assert_eq!(
            secret(Some(" written-down "), env_var).as_deref(),
            Some("written-down"),
            "what the config says wins, trimmed"
        );
        assert_eq!(
            secret(Some("   "), env_var).as_deref(),
            Some("from-the-environment"),
            "a setting that is only whitespace is not a setting, and the trimmed environment answers"
        );
        assert_eq!(
            secret(None, env_var).as_deref(),
            Some("from-the-environment")
        );

        unsafe { std::env::set_var(env_var, "  ") };
        assert_eq!(
            secret(None, env_var),
            None,
            "an environment variable that holds nothing holds nothing"
        );

        unsafe { std::env::remove_var(env_var) };
        assert_eq!(secret(None, env_var), None);
    }
}
