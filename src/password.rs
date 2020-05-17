use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};

pub fn generate() -> String {
    let rand_string: String =
        thread_rng().sample_iter(&Alphanumeric).take(30).collect();

    return rand_string;
}
