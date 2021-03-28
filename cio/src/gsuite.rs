use std::collections::BTreeMap;
use std::collections::HashMap;
use std::{thread, time};

use gsuite_api::{
    generate_password, Building as GSuiteBuilding, BuildingAddress, CalendarResource as GSuiteCalendarResource, GSuite, Group as GSuiteGroup, User as GSuiteUser, UserAddress, UserCustomProperties,
    UserEmail, UserGender, UserInstantMessenger, UserLocation, UserName, UserPhone, UserSSHKey,
};
use serde_json::Value;
use tracing::{event, instrument, Level};

use crate::configs::{Building, ConferenceRoom, Group};
use crate::utils::GSUITE_DOMAIN;

/// Update a group's aliases in GSuite to match our configuration files.
#[instrument(skip(gsuite))]
#[inline]
pub async fn update_group_aliases(gsuite: &GSuite, g: &GSuiteGroup) {
    if g.aliases.is_empty() {
        // return early
        return;
    }

    // Update the groups aliases.
    gsuite.update_group_aliases(&g.email, g.aliases.clone()).await;
    event!(Level::INFO, "updated gsuite group aliases: {}", g.email);
}

/// Update a group's settings in GSuite to match our configuration files.
#[instrument(skip(gsuite))]
#[inline]
pub async fn update_google_group_settings(gsuite: &GSuite, group: &Group) {
    // Get the current group settings.
    let email = format!("{}@{}", group.name, GSUITE_DOMAIN);
    let mut result = gsuite.get_group_settings(&email).await;
    if result.is_err() {
        // Try again.
        thread::sleep(time::Duration::from_secs(1));
        result = gsuite.get_group_settings(&email).await;
    }
    let mut settings = result.unwrap();

    // Update the groups settings.
    settings.email = email;
    settings.name = group.name.to_string();
    settings.description = group.description.to_string();
    settings.allow_external_members = group.allow_external_members.to_string();
    settings.allow_web_posting = group.allow_web_posting.to_string();
    settings.is_archived = group.is_archived.to_string();
    settings.who_can_discover_group = group.who_can_discover_group.to_string();
    settings.who_can_join = group.who_can_join.to_string();
    settings.who_can_moderate_members = group.who_can_moderate_members.to_string();
    settings.who_can_post_message = group.who_can_post_message.to_string();
    settings.who_can_view_group = group.who_can_view_group.to_string();
    settings.who_can_view_membership = group.who_can_view_membership.to_string();
    settings.who_can_contact_owner = "ALL_IN_DOMAIN_CAN_CONTACT".to_string();
    settings.enable_collaborative_inbox = group.enable_collaborative_inbox.to_string();

    // Update the group with the given settings.
    let result2 = gsuite.update_group_settings(&settings).await;
    if result2.is_err() {
        // Try again.
        thread::sleep(time::Duration::from_secs(1));
        gsuite.update_group_settings(&settings).await.unwrap();
    }

    event!(Level::INFO, "updated gsuite groups settings {}", group.name);
}

/// Update a building in GSuite.
#[instrument]
#[inline]
pub fn update_gsuite_building(b: &GSuiteBuilding, building: &Building, id: &str) -> GSuiteBuilding {
    let mut gsuite_building = b.clone();

    gsuite_building.id = id.to_string();
    gsuite_building.name = building.name.to_string();
    gsuite_building.description = building.description.to_string();
    gsuite_building.address = BuildingAddress {
        address_lines: vec![building.street_address.to_string()],
        locality: building.city.to_string(),
        administrative_area: building.state.to_string(),
        postal_code: building.zipcode.to_string(),
        region_code: building.country.to_string(),
        language_code: "en".to_string(),
        sublocality: "".to_string(),
    };
    gsuite_building.floor_names = building.floors.clone();

    gsuite_building
}

/// Update a calendar resource.
#[instrument]
#[inline]
pub fn update_gsuite_calendar_resource(c: &GSuiteCalendarResource, resource: &ConferenceRoom, id: &str) -> GSuiteCalendarResource {
    let mut gsuite_conference_room = c.clone();

    gsuite_conference_room.id = id.to_string();
    gsuite_conference_room.typev = resource.typev.to_string();
    gsuite_conference_room.name = resource.name.to_string();
    gsuite_conference_room.building_id = resource.building.to_string();
    gsuite_conference_room.description = resource.description.to_string();
    gsuite_conference_room.user_visible_description = resource.description.to_string();
    gsuite_conference_room.capacity = Some(resource.capacity);
    gsuite_conference_room.floor_name = resource.floor.to_string();
    gsuite_conference_room.floor_section = resource.section.to_string();
    gsuite_conference_room.category = "CONFERENCE_ROOM".to_string();

    gsuite_conference_room
}
