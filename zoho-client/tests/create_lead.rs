use serde_json::json;
use zoho_api::{client, client::ModuleUpdateResponseEntryDetails, modules};

// This test requires manual intervention to run. Permanently deleting a lead from Zoho requires
// an admin to delete the lead from the Zoho Recycling Bin (via the web ui). Until this delete is
// performed, any test run after the first will fail with a "duplicate data" error. This could be
// changed to generate a random value for the AirTable record id per run, but given that this
// requires write credentials to run as-is, it is left to the operator to perform the deletes.

#[ignore]
#[tokio::test]
async fn test_create_lead() {
    tracing_subscriber::fmt::init();

    let client = client::Zoho::new_from_env();
    client
        .refresh_access_token()
        .await
        .expect("Failed to refresh access token");

    let leads = client.module_client::<modules::Leads>();
    let notes = client.module_client::<modules::Notes>();

    let mut input = modules::LeadsInput::default();

    input.first_name = Some("Test".to_string());
    input.last_name = "Lead".to_string();
    input.email = Some("test_lead@oxidecomputer.com".to_string());
    input.company = Some("Oxide Computer".to_string());
    input.no_of_employees = Some(1);
    input.lead_source = Some("Automated Tests".to_string());
    input.submitted_interest = Some("Style!".to_string());
    input.airtable_lead_record_id = Some("12345".to_string());

    let tag = json!({
        "name": "Test Tag"
    });

    input.tag = Some(vec![tag]);

    let insert = leads.insert(vec![input.clone(), input], Some(vec![])).await.unwrap();

    assert_eq!("record added", insert.data[0].message);
    assert_eq!("success", insert.data[0].status);

    assert_eq!("duplicate data", insert.data[1].message);
    assert_eq!("error", insert.data[1].status);

    let record_id = match &insert.data[0].details {
        ModuleUpdateResponseEntryDetails::Success(details) => &details.id,
        _ => panic!("Failed to get a success response back for lead"),
    };

    let get = leads
        .get(record_id, client::GetModuleRecordsParams::default())
        .await
        .unwrap();

    assert_eq!(1, get.data.len());
    assert_eq!(record_id, &get.data[0].id);

    let mut note_input = modules::NotesInput::default();
    note_input.note_content = Some("Test attached notes".to_string());
    note_input.parent_id = serde_json::Value::String(record_id.to_string());
    note_input.se_module = "Leads".to_string();

    let inserted_note = notes.insert(vec![note_input], Some(vec![])).await.unwrap();

    assert_eq!("record added", inserted_note.data[0].message);
    assert_eq!("success", inserted_note.data[0].status);

    let note_id = match &inserted_note.data[0].details {
        ModuleUpdateResponseEntryDetails::Success(details) => &details.id,
        _ => panic!("Failed to get a success response back for note"),
    };

    let delete_note = notes.delete(vec![note_id], false).await.unwrap();

    assert_eq!("record deleted", delete_note.data[0].message);
    assert_eq!("success", delete_note.data[0].status);
    assert_eq!(note_id, &delete_note.data[0].details.id);

    let delete = leads.delete(vec![record_id], false).await.unwrap();

    assert_eq!("record deleted", delete.data[0].message);
    assert_eq!("success", delete.data[0].status);
    assert_eq!(record_id, &delete.data[0].details.id);
}
