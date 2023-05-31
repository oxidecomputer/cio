use std::{collections::HashMap, time};

use anyhow::{bail, Result};
use gsuite_api::{
    types::{
        Building as GSuiteBuilding, BuildingAddress, CalendarResource as GSuiteCalendarResource, Group as GSuiteGroup,
        Ims, User as GSuiteUser, UserAddress, UserEmail, UserGender, UserLocation, UserName, UserPhone,
        UserSshPublicKey,
    },
    Client as GSuite,
};
use log::info;
use serde_json::Value;

use crate::{
    companies::Company,
    configs::{Building, Group, Resource, User},
    db::Database,
    providers::{ProviderReadOps, ProviderWriteOps},
    utils::generate_password,
};

/// Update a GSuite user.
pub async fn update_gsuite_user(gu: &GSuiteUser, user: &User, change_password: bool, company: &Company) -> GSuiteUser {
    let mut gsuite_user = gu.clone();

    gsuite_user.name = Some(UserName {
        full_name: format!("{} {}", user.first_name, user.last_name),
        given_name: user.first_name.to_string(),
        family_name: user.last_name.to_string(),
    });

    if !user.recovery_email.is_empty() {
        // Set the recovery email for the user.
        gsuite_user.recovery_email = user.recovery_email.to_string();

        // Check if we have a home email set for the user and update it.
        let mut has_home_email = false;
        for (index, email) in gsuite_user.emails.iter().enumerate() {
            if email.type_ == "home" {
                // Update the set home email.
                gsuite_user.emails[index].address = user.recovery_email.to_string();
                // Break the loop early.
                has_home_email = true;
                break;
            }
        }

        if !has_home_email {
            // Set the home email for the user.
            gsuite_user.emails.push(UserEmail {
                custom_type: "".to_string(),
                type_: "home".to_string(),
                address: user.recovery_email.to_string(),
                primary: false,
            });
        }
    }

    if !user.recovery_phone.is_empty() {
        // Set the recovery phone for the user.
        gsuite_user.recovery_phone = user.recovery_phone.to_string();

        // Set the home phone for the user.
        gsuite_user.phones = vec![UserPhone {
            custom_type: "".to_string(),
            type_: "home".to_string(),
            value: user.recovery_phone.to_string(),
            primary: true,
        }];
    }

    gsuite_user.primary_email = user.email.to_string();

    if change_password {
        // Since we are creating a new user, we want to change their password
        // at the next login.
        gsuite_user.change_password_at_next_login = true;
        // Generate a password for the user.
        let password = generate_password();
        gsuite_user.password = password;
    }

    // Set the user's address if we have one.
    if !user.home_address_street_1.is_empty() {
        // TODO: this code is duplicated in configs.rs find a way to make it DRY.
        let mut street_address = user.home_address_street_1.to_string();
        if !user.home_address_street_2.is_empty() {
            street_address = format!("{}\n{}", user.home_address_street_1, user.home_address_street_2,);
        }
        gsuite_user.addresses = vec![UserAddress {
            country: user.home_address_country.to_string(),
            // TODO: fix this when we have an employee from another country.
            country_code: "US".to_string(),
            custom_type: "".to_string(),
            extended_address: "".to_string(),
            formatted: user.home_address_formatted.to_string(),
            locality: user.home_address_city.to_string(),
            po_box: "".to_string(),
            postal_code: user.home_address_zipcode.to_string(),
            primary: true,
            region: user.home_address_state.to_string(),
            // Indicates if the user-supplied address was formatted. Formatted addresses are
            // not currently supported.
            // FROM: https://developers.google.com/admin-sdk/directory/v1/reference/users#resource
            // TODO: figure out when this is supported and what it means
            source_is_structured: false,
            street_address,
            type_: "home".to_string(),
        }];
    }

    // Include the user in the global address list
    gsuite_user.include_in_global_address_list = true;

    if !user.gender.is_empty() {
        if user.gender == "male"
            || user.gender
                == "female
"
        {
            gsuite_user.gender = Some(UserGender {
                address_me_as: "".to_string(),
                custom_gender: "".to_string(),
                type_: user.gender.to_string(),
            });
        } else {
            gsuite_user.gender = Some(UserGender {
                address_me_as: "".to_string(),
                custom_gender: user.gender.to_string(),
                type_: "other".to_string(),
            });
        }
    } else {
        gsuite_user.gender = Some(UserGender {
            address_me_as: "".to_string(),
            custom_gender: "".to_string(),
            type_: "unknown".to_string(),
        });
    }

    if !user.building.is_empty() {
        gsuite_user.locations = vec![UserLocation {
            area: user.building.to_string(),
            building_id: user.building.to_string(),
            custom_type: "".to_string(),
            desk_code: "".to_string(),
            floor_name: "1".to_string(),
            floor_section: "".to_string(),
            type_: "default".to_string(),
        }];
    }

    // Set their GitHub SSH Keys to their Google SSH Keys.
    // Clear out their existing keys first.
    gsuite_user.ssh_public_keys = Default::default();
    for k in &user.public_ssh_keys {
        gsuite_user.ssh_public_keys.push(UserSshPublicKey {
            key: k.to_string(),
            expiration_time_usec: 0, // 0 will send empty
            // fingerprint is a read-only property so make sure it is empty
            fingerprint: "".to_string(),
        });
    }

    // Set the IM field for matrix.
    // TODO: once we migrate to slack update or add to this.
    if !user.chat.is_empty() {
        gsuite_user.ims = vec![
            Ims {
                custom_protocol: "matrix".to_string(),
                custom_type: "".to_string(),
                im: user.chat.to_string(),
                primary: true,
                protocol: "custom_protocol".to_string(),
                type_: "work".to_string(),
            },
            Ims {
                custom_protocol: "slack".to_string(),
                custom_type: "".to_string(),
                im: format!("@{}", user.github),
                primary: false,
                protocol: "custom_protocol".to_string(),
                type_: "work".to_string(),
            },
        ];
    }

    // Set the custom schemas.
    gsuite_user.custom_schemas = HashMap::new();
    let mut contact: HashMap<String, Value> = HashMap::new();
    contact.insert("Start_Date".to_string(), json!(user.start_date));

    // Set the GitHub username.
    if !user.github.is_empty() {
        contact.insert("GitHub_Username".to_string(), json!(user.github.to_string()));
    }
    // Oxide got set up weird but all the rest should be under miscellaneous.
    if company.name == "Oxide" {
        gsuite_user.custom_schemas.insert("Contact".to_string(), contact);
    } else {
        gsuite_user.custom_schemas.insert("Miscellaneous".to_string(), contact);
    }

    // Get the AWS Role information.
    if !user.aws_role.is_empty() {
        let mut aws_role: HashMap<String, Value> = HashMap::new();
        let mut aws_type: HashMap<String, String> = HashMap::new();
        aws_type.insert("type".to_string(), "work".to_string());
        aws_type.insert("value".to_string(), user.aws_role.to_string());
        aws_role.insert("Role".to_string(), json!(vec![aws_type]));
        gsuite_user
            .custom_schemas
            .insert("Amazon_Web_Services".to_string(), aws_role);
    }

    gsuite_user
}

/// Update a user's aliases in GSuite to match our database.
pub async fn update_user_aliases(
    gsuite: &GSuite,
    u: &GSuiteUser,
    aliases: Vec<String>,
    company: &Company,
) -> Result<()> {
    if aliases.is_empty() {
        // Return early.
        return Ok(());
    }

    let mut formatted_aliases: Vec<String> = Default::default();
    for a in aliases {
        formatted_aliases.push(format!("{}@{}", a, company.gsuite_domain));
    }

    // Update the user's aliases.
    for alias in formatted_aliases {
        match gsuite
            .users()
            .aliases_insert(
                &u.primary_email,
                &gsuite_api::types::Alias {
                    alias: alias.to_string(),
                    etag: Default::default(),
                    id: Default::default(),
                    kind: Default::default(),
                    primary_email: Default::default(),
                },
            )
            .await
        {
            Ok(_) => (),
            Err(e) => {
                if e.to_string().contains("Entity already exists") {
                    // Ignore the error.
                    continue;
                }
                bail!("updating gsuite user {} aliases failed: {}", u.primary_email, e);
            }
        }
    }

    info!("updated GSuite user `{}` aliases", u.primary_email);
    Ok(())
}

/// Update a user's groups in GSuite to match our database.
pub async fn update_user_google_groups(gsuite: &GSuite, user: &User, company: &Company) -> Result<()> {
    // Get all the GSuite groups.
    let gsuite_groups = gsuite.list_provider_groups(company).await?;

    // Iterate over the groups and add the user as a member to it.
    for group in &user.groups {
        // Ensure that this is a valid group before performing operations
        if let Some(gsuite_group) = gsuite_groups.iter().find(|g| &g.name == group) {
            gsuite.add_user_to_group(company, user, &gsuite_group.name).await?;
        }
    }

    // Iterate over all the groups and if the user is a member and should not
    // be, remove them from the group.
    for group in &gsuite_groups {
        if user.groups.contains(&group.name) {
            // They should be in the group, continue.
            continue;
        }

        // Now we have a github team. The user should not be a member of it,
        // but we need to make sure they are not a member.
        let is_member = gsuite.check_user_is_member_of_group(company, user, &group.name).await?;

        // They are a member of the team.
        // We need to remove them.
        if is_member {
            gsuite.remove_user_from_group(company, user, &group.name).await?;
        }
    }

    Ok(())
}

/// Update a group's aliases in GSuite to match our configuration files.
pub async fn update_group_aliases(gsuite: &GSuite, g: &GSuiteGroup) -> Result<()> {
    if g.aliases.is_empty() {
        // return early
        return Ok(());
    }

    // Update the user's aliases.
    for alias in &g.aliases {
        match gsuite
            .groups()
            .aliases_insert(
                &g.email,
                &gsuite_api::types::Alias {
                    alias: alias.to_string(),
                    etag: Default::default(),
                    id: Default::default(),
                    kind: Default::default(),
                    primary_email: Default::default(),
                },
            )
            .await
        {
            Ok(_) => (),
            Err(e) => {
                if e.to_string().contains("Entity already exists") {
                    // Ignore the error.
                    continue;
                }
                bail!("updating gsuite group {} aliases failed: {}", g.email, e);
            }
        }
    }

    info!("updated gsuite group aliases: {}", g.email);
    Ok(())
}

/// Update a group's settings in GSuite to match our configuration files.
pub async fn update_google_group_settings(db: &Database, group: &Group, company: &Company) -> Result<()> {
    let ggs = company.authenticate_google_groups_settings(db).await?;

    // Get the current group settings.
    let email = format!("{}@{}", group.name, company.gsuite_domain);
    let mut result = ggs.groups().get(google_groups_settings::types::Alt::Json, &email).await;
    if result.is_err() {
        // Try again.
        tokio::time::sleep(time::Duration::from_secs(1)).await;
        result = ggs.groups().get(google_groups_settings::types::Alt::Json, &email).await;
    }
    let mut settings = result?.body;

    // Update the groups settings.
    settings.email = email.to_string();
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

    // Update the group with the given settings.
    let result2 = ggs
        .groups()
        .update(google_groups_settings::types::Alt::Json, &email, &settings)
        .await;
    if result2.is_err() {
        // Try again.
        tokio::time::sleep(time::Duration::from_secs(1)).await;
        ggs.groups()
            .update(google_groups_settings::types::Alt::Json, &email, &settings)
            .await?;
    }

    info!("updated gsuite groups settings {}", group.name);

    Ok(())
}

/// Update a building in GSuite.
pub fn update_gsuite_building(b: &GSuiteBuilding, building: &Building, id: &str) -> GSuiteBuilding {
    let mut gsuite_building = b.clone();

    gsuite_building.building_id = id.to_string();
    gsuite_building.building_name = building.name.to_string();
    gsuite_building.description = building.description.to_string();
    gsuite_building.address = Some(BuildingAddress {
        address_lines: vec![building.street_address.to_string()],
        locality: building.city.to_string(),
        administrative_area: building.state.to_string(),
        postal_code: building.zipcode.to_string(),
        region_code: building.country.to_string(),
        language_code: "en".to_string(),
        sublocality: "".to_string(),
    });
    gsuite_building.floor_names = building.floors.clone();

    gsuite_building
}

/// Update a calendar resource.
pub fn update_gsuite_calendar_resource(
    c: &GSuiteCalendarResource,
    resource: &Resource,
    id: &str,
) -> GSuiteCalendarResource {
    let mut gsuite_resource = c.clone();

    gsuite_resource.resource_id = id.to_string();
    gsuite_resource.resource_type = resource.typev.to_string();
    gsuite_resource.resource_name = resource.name.to_string();
    gsuite_resource.building_id = resource.building.to_string();
    gsuite_resource.resource_description = resource.description.to_string();
    gsuite_resource.user_visible_description = resource.description.to_string();
    gsuite_resource.capacity = resource.capacity as i64;
    gsuite_resource.floor_name = resource.floor.to_string();
    gsuite_resource.floor_section = resource.section.to_string();
    gsuite_resource.resource_category = resource.category.to_api_value();

    gsuite_resource
}
