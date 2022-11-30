pub struct Printer;

impl Printer {
    pub fn key() -> String {
        std::env::var("PRINT_TOKEN").unwrap_or_else(|_| "".to_string())
    }
}
