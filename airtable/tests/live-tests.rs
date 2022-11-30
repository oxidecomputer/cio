use airtable_api::{Airtable, scim::{ScimClientError, group::{ScimGroupIndex, ScimCreateGroup, ScimUpdateGroup}}};

#[cfg(feature = "live-tests")]
#[tokio::test]
async fn test_read_scim_users() {
    let test_id = std::env::var("TEST_USER_ID").expect("Failed to find test user id");

    let airtable = Airtable::new_from_env();
    let scim = airtable.scim();
    let users = scim.user();

    let user = users.get(&test_id).await.unwrap();
    let mut user_list = users.list(None).await.unwrap();
    
    user_list.resources.retain(|u| u.id == test_id);

    let test_user = user_list.resources.into_iter().next();

    assert!(user.is_some());

    assert_eq!(user, test_user);
}

#[cfg(feature = "live-tests")]
#[tokio::test]
async fn test_update_scim_users() {

}

#[cfg(feature = "live-tests")]
#[tokio::test]
async fn test_read_scim_groups() {
    let test_id = std::env::var("TEST_GROUP_ID").expect("Failed to find test group id");

    let airtable = Airtable::new_from_env();
    let scim = airtable.scim();
    let groups = scim.group();

    let group = groups.get(&test_id).await.unwrap();

    let mut group_list = groups.list(None).await.unwrap();
    
    group_list.resources.retain(|u| u.id == test_id);

    let test_group = group_list.resources.into_iter().next();

    assert!(group.is_some());

    let index_group: Option<ScimGroupIndex> = group.map(|g| g.into());

    assert_eq!(index_group, test_group);
}

#[cfg(feature = "live-tests")]
#[tokio::test]
async fn test_create_delete_groups() {
    let airtable = Airtable::new_from_env();
    let scim = airtable.scim();
    let groups = scim.group();

    let group = groups.create(&ScimCreateGroup {
        schemas: vec!["urn:ietf:params:scim:schemas:core:2.0:Group".to_string()],
        display_name: "Enterprise API Integration Test Temporary Group".to_string(),
    }).await.unwrap();

    let id = group.id;

    let updated_group = groups.update(
        &id,
        &ScimUpdateGroup {
            schemas: None,
            display_name: Some("Enterprise API Integration Test Temporary Group Post-Update".to_string()),
            members: None
        }
    ).await.unwrap();

    assert_eq!(
        "Enterprise API Integration Test Temporary Group Post-Update".to_string(),
        updated_group.display_name
    );

    groups.delete(updated_group.id).await.unwrap();

    let expected_missing = groups.get(&id).await;

    let error = expected_missing.unwrap_err();

    match error {
        ScimClientError::Api(inner) => {
            assert_eq!(404, inner.status);
        }
        other => panic!("Expected to receive a 404 error for a deleted group, but instead found {:?}", other)
    }
}