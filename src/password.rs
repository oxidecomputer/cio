use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};

/// Generate a random string that we can use as a temporary password for new users
/// when we set up their account.
pub fn generate() -> String {
    let rand_string: String =
        thread_rng().sample_iter(&Alphanumeric).take(30).collect();

    return rand_string;
}
