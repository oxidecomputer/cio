use crate::companies::Company;
use crate::db::Database;

pub async fn refresh_trip_actions(db: &Database, company: &Company) {
    // Authenticate with TripActions.
    let ta = company.authenticate_tripactions(db);

    // Let's get our bookings.
    let bookings = ta.get_bookings().await.unwrap();
    for booking in bookings {
        println!("Booking: {:?}", booking);
    }
}

#[cfg(test)]
mod tests {
    use crate::companies::Company;
    use crate::db::Database;
    use crate::travel::refresh_trip_actions;

    #[ignore]
    #[tokio::test(flavor = "multi_thread")]
    async fn test_travel() {
        let db = Database::new();

        // Get the company id for Oxide.
        // TODO: split this out per company.
        let oxide = Company::get_from_db(&db, "Oxide".to_string()).unwrap();

        refresh_trip_actions(&db, &oxide).await;
    }
}
