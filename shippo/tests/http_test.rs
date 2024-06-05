#![recursion_limit = "256"]

use httpmock::MockServer;
use reqwest::Url;

use shippo::Shippo;

mod data;

#[tokio::test]
async fn list_shipments() {
    let server = MockServer::start();
    let client = Shippo::new("token123");
    let mock_url = Url::parse(&server.base_url()).unwrap();
    let mock = server.mock(|when, then| {
        when.method("GET")
            .path("/shipments")
            .header("authorization", String::from("ShippoToken token123"))
            .header("Content-Type", "application/json");
        then.status(200).json_body(data::list_shipments().json_body);
    });

    // Act
    let result = client.base_url(mock_url).list_shipments().await;

    // Assert
    mock.assert();
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.len(), 1);
    assert_eq!(response[0], data::list_shipments().deserialized[0])
}

#[tokio::test]
async fn create_shipment() {
    // Arrange
    let server = MockServer::start();
    let mock_url = Url::parse(&server.base_url()).unwrap();
    let mock = server.mock(|when, then| {
        when.method("POST")
            .path("/shipments")
            .header("authorization", String::from("ShippoToken token123"))
            .header("Content-Type", "application/json");
        then.status(201).json_body(data::create_shipment().response.json_body);
    });

    // Act
    let client = Shippo::new("token123");
    let result = client
        .base_url(mock_url)
        .create_shipment(data::create_shipment().body)
        .await;

    // Assert
    mock.assert();
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response, data::create_shipment().response.deserialized)
}

#[tokio::test]
async fn get_shipment() {
    // Arrange
    let shipment_id = String::from("7c47d12aa95a4cbb9d90c167cca7bea7");
    let server = MockServer::start();
    let mock_url = Url::parse(&server.base_url()).unwrap();
    let mock = server.mock(|when, then| {
        when.method("GET")
            .path(format!("/shipments/{}", shipment_id))
            .header("authorization", String::from("ShippoToken token123"))
            .header("Content-Type", "application/json");
        then.status(200).json_body(data::get_shipment().json_body);
    });

    // Act
    let client = Shippo::new("token123");
    let result = client.base_url(mock_url).get_shipment(&shipment_id).await;

    // Assert
    mock.assert();
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response, data::get_shipment().deserialized)
}

#[tokio::test]
async fn get_rate() {
    // Arrange
    let rate_id = String::from("545ab0a1a6ea4c9f9adb2512a57d6d8b");
    let server = MockServer::start();
    let mock_url = Url::parse(&server.base_url()).unwrap();
    let mock = server.mock(|when, then| {
        when.method("GET")
            .path(format!("/rates/{}", rate_id))
            .header("authorization", String::from("ShippoToken token123"))
            .header("Content-Type", "application/json");
        then.status(200).json_body(data::get_rate().json_body);
    });

    // Act
    let client = Shippo::new("token123");
    let result = client.base_url(mock_url).get_rate(&rate_id).await;

    // Assert
    mock.assert();
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response, data::get_rate().deserialized)
}

#[tokio::test]
async fn create_pickup() {
    // Arrange
    let server = MockServer::start();
    let mock_url = Url::parse(&server.base_url()).unwrap();
    let mock = server.mock(|when, then| {
        when.method("POST")
            .path("/pickups/")
            .header("authorization", String::from("ShippoToken token123"))
            .header("Content-Type", "application/json");
        then.status(201).json_body(data::create_pickup().response.json_body);
    });

    // Act
    let client = Shippo::new("token123");
    let result = client
        .base_url(mock_url)
        .create_pickup(&data::create_pickup().body)
        .await;

    // Assert
    mock.assert();
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response, data::create_pickup().response.deserialized)
}

#[tokio::test]
async fn create_customs_item() {
    // Arrange
    let server = MockServer::start();
    let mock_url = Url::parse(&server.base_url()).unwrap();
    let mock = server.mock(|when, then| {
        when.method("POST")
            .path("/customs/items/")
            .header("authorization", String::from("ShippoToken token123"))
            .header("Content-Type", "application/json");
        then.status(201)
            .json_body(data::create_customs_item().response.json_body);
    });

    // Act
    let client = Shippo::new("token123");
    let result = client
        .base_url(mock_url)
        .create_customs_item(data::create_customs_item().body)
        .await;

    // Assert
    mock.assert();
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response, data::create_customs_item().response.deserialized)
}

#[tokio::test]
async fn create_shipping_label_from_rate() {
    // Arrange
    let server = MockServer::start();
    let mock_url = Url::parse(&server.base_url()).unwrap();
    let mock = server.mock(|when, then| {
        when.method("POST")
            .path("/transactions")
            .header("authorization", String::from("ShippoToken token123"))
            .header("Content-Type", "application/json");
        then.status(201)
            .json_body(data::create_shipping_label_from_rate().response.json_body);
    });

    // Act
    let client = Shippo::new("token123");
    let result = client
        .base_url(mock_url)
        .create_shipping_label_from_rate(data::create_shipping_label_from_rate().body)
        .await;

    // Assert
    mock.assert();
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response, data::create_shipping_label_from_rate().response.deserialized)
}

#[tokio::test]
async fn get_shipping_label() {
    // Arrange
    let shipping_id = String::from("70ae8117ee1749e393f249d5b77c45e0");
    let server = MockServer::start();
    let mock_url = Url::parse(&server.base_url()).unwrap();
    let mock = server.mock(|when, then| {
        when.method("GET")
            .path(format!("/transactions/{}", shipping_id))
            .header("authorization", String::from("ShippoToken token123"))
            .header("Content-Type", "application/json");
        then.status(200).json_body(data::get_shipping_label().json_body);
    });

    // Act
    let client = Shippo::new("token123");
    let result = client.base_url(mock_url).get_shipping_label(&shipping_id).await;

    // Assert
    mock.assert();
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response, data::get_shipping_label().deserialized)
}

#[tokio::test]
async fn list_shipping_labels() {
    // Arrange
    let server = MockServer::start();
    let mock_url = Url::parse(&server.base_url()).unwrap();
    let mock = server.mock(|when, then| {
        when.method("GET")
            .path("/transactions")
            .header("authorization", String::from("ShippoToken token123"))
            .header("Content-Type", "application/json");
        then.status(200).json_body(data::list_shipping_labels().json_body);
    });

    // Act
    let client = Shippo::new("token123");
    let result = client.base_url(mock_url).list_shipping_labels().await;

    // Assert
    mock.assert();
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.len(), 1);
    assert_eq!(response[0], data::list_shipping_labels().deserialized[0])
}

#[tokio::test]
async fn register_tracking_webhook() {
    // Arrange
    let server = MockServer::start();
    let mock_url = Url::parse(&server.base_url()).unwrap();
    let mock = server.mock(|when, then| {
        when.method("POST")
            .path("/tracks")
            .header("authorization", String::from("ShippoToken token123"))
            .header("Content-Type", "application/json");
        then.status(201)
            .json_body(data::register_tracking_webhook().response.json_body);
    });

    // Act
    let client = Shippo::new("token123");
    let result = client
        .base_url(mock_url)
        .register_tracking_webhook(
            &data::register_tracking_webhook().body.0,
            &data::register_tracking_webhook().body.1,
        )
        .await;

    // Assert
    mock.assert();
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response, data::register_tracking_webhook().response.deserialized)
}

#[tokio::test]
async fn list_orders() {
    // Arrange
    let server = MockServer::start();
    let mock_url = Url::parse(&server.base_url()).unwrap();
    let mock = server.mock(|when, then| {
        when.method("GET")
            .path("/orders")
            .header("authorization", String::from("ShippoToken token123"))
            .header("Content-Type", "application/json");
        then.status(200).json_body(data::list_orders("").json_body);
    });

    // Act
    let client = Shippo::new("token123");
    let result = client.base_url(mock_url).list_orders().await;

    // Assert
    mock.assert();
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.len(), 1);
    assert_eq!(response[0], data::list_orders("").deserialized[0]);
}

#[tokio::test]
async fn list_orders_paginates() {
    // Arrange
    let server = MockServer::start();
    let mock_url = Url::parse(&server.base_url()).unwrap();
    let pages = server.mock(|when, then| {
        when.method("GET")
            .path("/orders")
            .header("authorization", String::from("ShippoToken token123"))
            .header("Content-Type", "application/json");
        then.status(200)
            .json_body(data::list_orders(format!("{}orders?page=2", mock_url)).json_body);
    });
    // Act
    let client = Shippo::new("token123");
    let result = client.base_url(mock_url).list_orders().await;

    // Assert
    pages.assert_hits(2);
    assert!(result.is_ok());
    let response = result.unwrap();
    // Expect length to be 2 because the first response returns a next page
    // and each page has 1 order for testing pagination
    assert_eq!(response.len(), 2);
    assert_eq!(response[0], data::list_orders("").deserialized[0]);
    assert_eq!(response[1], data::list_orders("").deserialized[0]);
}
#[tokio::test]
async fn get_tracking_status() {
    // Arrange
    let server = MockServer::start();
    let mock_url = Url::parse(&server.base_url()).unwrap();
    let carrier = String::from("usps");
    let tracking_number = String::from("9205590164917312751089");
    let mock = server.mock(|when, then| {
        when.method("GET")
            .path(format!("/tracks/{}/{}", carrier, tracking_number))
            .header("authorization", String::from("ShippoToken token123"))
            .header("Content-Type", "application/json");
        then.status(200).json_body(data::get_tracking_status().json_body);
    });

    // Act
    let client = Shippo::new("token123");
    let result = client
        .base_url(mock_url)
        .get_tracking_status(&carrier, &tracking_number)
        .await;

    // Assert
    mock.assert();
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response, data::get_tracking_status().deserialized)
}

#[tokio::test]
async fn list_carrier_accounts() {
    // Arrange
    let server = MockServer::start();
    let mock_url = Url::parse(&server.base_url()).unwrap();
    let mock = server.mock(|when, then| {
        when.method("GET")
            .path("/carrier_accounts")
            .header("authorization", String::from("ShippoToken token123"))
            .header("Content-Type", "application/json");
        then.status(200).json_body(data::list_carrier_accounts("").json_body);
    });

    // Act
    let client = Shippo::new("token123");
    let result = client.base_url(mock_url).list_carrier_accounts().await;

    // Assert
    mock.assert();
    assert!(result.is_ok());
    let response = result.unwrap();
    assert_eq!(response.len(), 1);
    assert_eq!(response[0], data::list_carrier_accounts("").deserialized[0]);
}

#[tokio::test]
async fn list_carrier_accounts_paginates() {
    // Arrange
    let server = MockServer::start();
    let mock_url = Url::parse(&server.base_url()).unwrap();
    let pages = server.mock(|when, then| {
        when.method("GET")
            .path("/carrier_accounts")
            .header("authorization", String::from("ShippoToken token123"))
            .header("Content-Type", "application/json");
        then.status(200)
            .json_body(data::list_carrier_accounts(format!("{}carrier_accounts?page=2", mock_url)).json_body);
    });

    // Act
    let client = Shippo::new("token123");
    let result = client.base_url(mock_url).list_carrier_accounts().await;

    // Assert

    pages.assert_hits(2);
    assert!(result.is_ok());
    let response = result.unwrap();
    // Expect length to be 2 because the first response returns a next page
    // and each page has 1 order for testing pagination
    assert_eq!(response.len(), 2);
    assert_eq!(response[0], data::list_carrier_accounts("").deserialized[0]);
    assert_eq!(response[1], data::list_carrier_accounts("").deserialized[0]);
}
