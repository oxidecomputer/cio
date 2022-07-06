pub struct Features;

impl Features {
    pub fn is_enabled<S>(feature: S) -> bool
    where
        S: AsRef<str>,
    {
        std::env::var(feature.as_ref())
            .map(|f| f.to_lowercase() == "true")
            .unwrap_or(false)
    }
}
