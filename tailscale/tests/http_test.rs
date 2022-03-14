use chrono::{offset::Utc, DateTime, NaiveDateTime};
use httpmock::MockServer;
use reqwest::Url;
use serde_json::json;

use tailscale_api::{Device, Tailscale};

#[tokio::test]
async fn list_devices_test() {
    let domain = "my.domain";
    let key = "key123";
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method("GET")
            .path(format!("/domain/{}/devices", domain))
            .header("authorization", String::from("Basic a2V5MTIzOg==")) // httpmock masks auth keys
            .header("Content-Type", "application/json");
        then.status(200).json_body(json!({ "devices":[
          {
            "addresses":[
              "100.68.203.125"
            ],
            "clientVersion":"date.20201107",
            "os":"macOS",
            "name":"user1-device.example.com",
            "created":"2020-11-30T22:20:04Z",
            "lastSeen":"2020-11-30T17:20:04+00:00",
            "hostname":"User1-Device",
            "machineKey":"mkey:user1-node-key",
            "nodeKey":"nodekey:user1-node-key",
            "id":"12345",
            "user":"user1@example.com",
            "expires":"2021-05-29T22:20:04Z",
            "keyExpiryDisabled":false,
            "authorized":false,
            "isExternal":false,
            "updateAvailable":false,
            "blocksIncomingConnections":false,
          }, // https://github.com/tailscale/tailscale/blob/main/api.md#-get-apiv2tailnettailnetdevices---list-the-devices-for-a-tailnet
        ]}));
    });
    let client = Tailscale::new(String::from(key), domain);

    let mock_url = Url::parse(&server.base_url()).unwrap();

    // Act
    let result = client.base_url(mock_url).list_devices().await;

    // Assert
    mock.assert();
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.len(), 1);
    assert_eq!(
        response[0],
        Device {
            addresses: vec![String::from("100.68.203.125")],
            allowed_ips: vec![],
            authorized: false,
            client_version: String::from("date.20201107"),
            created: DateTime::<Utc>::from_utc(
                NaiveDateTime::parse_from_str("2020-11-30T22:20:04Z", "%Y-%m-%dT%H:%M:%SZ").unwrap(),
                Utc
            ),
            derp: String::from(""),
            display_node_key: String::from(""),
            endpoints: vec![],
            expires: DateTime::<Utc>::from_utc(
                NaiveDateTime::parse_from_str("2021-05-29T22:20:04Z", "%Y-%m-%dT%H:%M:%SZ").unwrap(),
                Utc
            ),
            extra_ips: vec![],
            has_subnet: false,
            hostname: String::from("User1-Device"),
            id: String::from("12345"),
            is_external: false,
            last_seen: DateTime::<Utc>::from_utc(
                NaiveDateTime::parse_from_str("2020-11-30T17:20:04+00:00", "%Y-%m-%dT%H:%M:%S%z").unwrap(),
                Utc
            ),
            log_id: String::from(""),
            machine_key: String::from("mkey:user1-node-key"),
            name: String::from("user1-device.example.com"),
            never_expires: false,
            node_key: String::from("nodekey:user1-node-key"),
            os: String::from("macOS"),
            route_all: false,
            update_available: false,
            user: String::from("user1@example.com"),
        }
    );
}

#[tokio::test]
async fn delete_device_test() {
    let device = "12345";
    let domain = "my.domain";
    let key = "key123";
    let server = MockServer::start();
    let mock = server.mock(|when, then| {
        when.method("DELETE")
            .path(format!("/device/{}", &device))
            .header("authorization", String::from("Basic a2V5MTIzOg==")) // httpmock masks auth keys
            .header("Content-Type", "application/json");
        then.status(200);
    });
    let client = Tailscale::new(String::from(key), domain);

    let mock_url = Url::parse(&server.base_url()).unwrap();

    // Act
    let result = client.base_url(mock_url).delete_device(device).await;

    // Assert
    mock.assert();
    assert!(result.is_ok());
}
