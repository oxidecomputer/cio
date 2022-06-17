use cio_api::recorded_meetings::RecordedMeeting;

#[tokio::test]
async fn test_airtable_row_equivalence() {
    let db = cio_api::db::Database::new().await;
    let db_meeting = RecordedMeeting::get_by_id(&db, 1070).await.unwrap();
    let airtable_meeting = db_meeting.get_existing_airtable_record(&db).await.unwrap();

    assert_eq!(db_meeting, airtable_meeting.fields);
}
