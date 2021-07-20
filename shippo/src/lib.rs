/*!
 * A rust library for interacting with the Shippo API.
 *
 * For more information, the Shippo API is documented at [goshippo.com/docs/reference](https://goshippo.com/docs/reference).
 *
 * Example:
 *
 * ```
 * use serde::{Deserialize, Serialize};
 * use shippo::Shippo;
 *
 * async fn get_shipments() {
 *     // Initialize the Shippo client.
 *     let shippo = Shippo::new_from_env();
 *
 *     // List the shipments.
 *     let shipments = shippo.list_shipments().await.unwrap();
 *
 *     // Iterate over the shipments.
 *     for shipment in shipments {
 *         println!("{:?}", shipment);
 *     }
 * }
 * ```
 */
#![allow(clippy::field_reassign_with_default)]
use std::{
    borrow::Cow, collections::HashMap, env, error, fmt, fmt::Debug, str::FromStr, sync::Arc,
};

use chrono::{offset::Utc, DateTime};
use reqwest::{header, Client, Method, Request, StatusCode, Url};
use schemars::JsonSchema;
use serde::{
    de::{self, Visitor},
    Deserialize, Serialize,
};

/// Endpoint for the Shippo API.
const ENDPOINT: &str = "https://api.goshippo.com/";

/// Entrypoint for interacting with the Shippo API.
pub struct Shippo {
    token: String,

    client: Arc<Client>,
}

impl Shippo {
    /// Create a new Shippo client struct. It takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API Token your requests will work.
    pub fn new<K>(token: K) -> Self
    where
        K: ToString,
    {
        let client = Client::builder().build();
        match client {
            Ok(c) => Self {
                token: token.to_string(),

                client: Arc::new(c),
            },
            Err(e) => panic!("creating client failed: {:?}", e),
        }
    }

    /// Create a new Shippo client struct from environment variables. It
    /// takes a type that can convert into
    /// an &str (`String` or `Vec<u8>` for example). As long as the function is
    /// given a valid API Token and your requests will work.
    pub fn new_from_env() -> Self {
        let token = env::var("SHIPPO_API_TOKEN").unwrap();

        Shippo::new(token)
    }

    fn request<B>(
        &self,
        method: Method,
        path: &str,
        body: B,
        query: Option<Vec<(String, String)>>,
    ) -> Request
    where
        B: Serialize,
    {
        let base = Url::parse(ENDPOINT).unwrap();
        let url = base.join(path).unwrap();

        let bt = format!("ShippoToken {}", self.token);
        let bearer = header::HeaderValue::from_str(&bt).unwrap();

        // Set the default headers.
        let mut headers = header::HeaderMap::new();
        headers.append(header::AUTHORIZATION, bearer);
        headers.append(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );

        let mut rb = self.client.request(method.clone(), url).headers(headers);

        match query {
            None => (),
            Some(val) => {
                rb = rb.query(&val);
            }
        }

        // Add the body, this is to ensure our GET and DELETE calls succeed.
        if method != Method::GET && method != Method::DELETE {
            rb = rb.json(&body);
        }

        // Build the request.
        rb.build().unwrap()
    }

    /// List shipments.
    /// FROM: https://goshippo.com/docs/reference#shipments-list
    /// A maximum date range of 90 days is permitted. Provided dates should be ISO 8601 UTC dates.
    pub async fn list_shipments(&self) -> Result<Vec<Shipment>, APIError> {
        // Build the request.
        // TODO: paginate.
        let request = self.request(Method::GET, "shipments", (), None);

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        let r: APIResponse = resp.json().await.unwrap();

        Ok(r.shipments)
    }

    /// Create a shipment.
    /// FROM: https://goshippo.com/docs/reference#shipments-create
    pub async fn create_shipment(&self, ns: NewShipment) -> Result<Shipment, APIError> {
        // Build the request.
        let request = self.request(Method::POST, "shipments", ns, None);

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::CREATED => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        Ok(resp.json().await.unwrap())
    }

    /// List the orders.
    /// FROM: https://goshippo.com/docs/reference#orders-list
    pub async fn list_orders(&self) -> Result<Vec<Order>, APIError> {
        // Build the request.
        let mut request = self.request(Method::GET, "orders", (), None);

        let mut resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        let mut r: OrdersAPIResponse = resp.json().await.unwrap();
        let mut orders = r.orders;
        let mut page = r.next;

        // Paginate if we should.
        // TODO: make this more DRY
        while !page.is_empty() {
            let url = Url::parse(&page).unwrap();
            let pairs: Vec<(Cow<'_, str>, Cow<'_, str>)> = url.query_pairs().collect();
            let mut new_pairs: Vec<(String, String)> = Vec::new();
            for (a, b) in pairs {
                let sa = a.into_owned();
                let sb = b.into_owned();
                new_pairs.push((sa, sb));
            }

            request = self.request(Method::GET, "orders", (), Some(new_pairs));

            resp = self.client.execute(request).await.unwrap();
            match resp.status() {
                StatusCode::OK => (),
                s => {
                    return Err(APIError {
                        status_code: s,
                        body: resp.text().await.unwrap(),
                    })
                }
            };

            // Try to deserialize the response.
            r = resp.json().await.unwrap();

            orders.append(&mut r.orders);

            if !r.next.is_empty() && r.next != page {
                page = r.next;
            } else {
                page = "".to_string();
            }
        }

        Ok(orders)
    }

    /// List the carrier accounts.
    /// FROM: https://goshippo.com/docs/reference#carrier-accounts
    pub async fn list_carrier_accounts(&self) -> Result<Vec<CarrierAccount>, APIError> {
        // Build the request.
        let mut request = self.request(Method::GET, "carrier_accounts", (), None);

        let mut resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        let mut r: CarrierAccountsAPIResponse = resp.json().await.unwrap();
        let mut carrier_accounts = r.carrier_accounts;
        let mut page = r.next;

        // Paginate if we should.
        // TODO: make this more DRY
        while !page.is_empty() {
            let url = Url::parse(&page).unwrap();
            let pairs: Vec<(Cow<'_, str>, Cow<'_, str>)> = url.query_pairs().collect();
            let mut new_pairs: Vec<(String, String)> = Vec::new();
            for (a, b) in pairs {
                let sa = a.into_owned();
                let sb = b.into_owned();
                new_pairs.push((sa, sb));
            }

            request = self.request(Method::GET, "carrier_accounts", (), Some(new_pairs));

            resp = self.client.execute(request).await.unwrap();
            match resp.status() {
                StatusCode::OK => (),
                s => {
                    return Err(APIError {
                        status_code: s,
                        body: resp.text().await.unwrap(),
                    })
                }
            };

            // Try to deserialize the response.
            r = resp.json().await.unwrap();

            carrier_accounts.append(&mut r.carrier_accounts);

            if !r.next.is_empty() && r.next != page {
                page = r.next;
            } else {
                page = "".to_string();
            }
        }

        Ok(carrier_accounts)
    }

    /// Get a shipment.
    /// FROM: https://goshippo.com/docs/reference#shipments-retrieve
    pub async fn get_shipment(&self, id: &str) -> Result<Shipment, APIError> {
        // Build the request.
        let request = self.request(Method::GET, &format!("shipments/{}", id), (), None);

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        Ok(resp.json().await.unwrap())
    }

    /// Create a pickup.
    /// FROM: https://goshippo.com/docs/reference#pickups-create
    pub async fn create_pickup(&self, np: &NewPickup) -> Result<Pickup, APIError> {
        // Build the request.
        let request = self.request(Method::POST, "pickups/", np, None);

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::CREATED => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        Ok(resp.json().await.unwrap())
    }

    /// Create a customs item.
    /// FROM: https://goshippo.com/docs/reference#customs-items-create
    pub async fn create_customs_item(&self, c: CustomsItem) -> Result<CustomsItem, APIError> {
        // Build the request.
        let request = self.request(Method::POST, "customs/items/", c, None);

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::CREATED => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        Ok(resp.json().await.unwrap())
    }

    /// Create a shipping label based on a rate.
    /// FROM: https://goshippo.com/docs/reference#transactions-create
    pub async fn create_shipping_label_from_rate(
        &self,
        nt: NewTransaction,
    ) -> Result<Transaction, APIError> {
        // Build the request.
        let request = self.request(Method::POST, "transactions", nt, None);

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::CREATED => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        Ok(resp.json().await.unwrap())
    }

    /// Get a shipping label.
    /// FROM: https://goshippo.com/docs/reference#transactions-retrieve
    pub async fn get_shipping_label(&self, id: &str) -> Result<Transaction, APIError> {
        // Build the request.
        let request = self.request(Method::GET, &format!("transactions/{}", id), (), None);

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        let status = &resp.status();

        match &resp.json::<Transaction>().await {
            Ok(v) => Ok(v.clone()),
            Err(e) => {
                return Err(APIError {
                    status_code: *status,
                    // TODO: somehow get the body
                    body: format!("{}", e),
                });
            }
        }
    }

    /// List shiping labels.
    /// FROM: https://goshippo.com/docs/reference#transactions-list
    pub async fn list_shipping_labels(&self) -> Result<Vec<Transaction>, APIError> {
        // Build the request.
        // TODO: paginate.
        let request = self.request(Method::GET, "transactions", (), None);

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        let r: TransactionsAPIResponse = resp.json().await.unwrap();

        Ok(r.transactions)
    }

    /// Register a tracking webhook.
    /// You can register your webhook(s) for a Shipment (and request the current status at the same time)
    /// by POSTing to the tracking endpoint. This way Shippo will send HTTP notifications to your
    /// track_updated webhook(s) whenever the status changes.
    /// FROM: https://goshippo.com/docs/reference#tracks-create
    pub async fn register_tracking_webhook(
        &self,
        carrier: &str,
        tracking_number: &str,
    ) -> Result<TrackingStatus, APIError> {
        let mut body: HashMap<&str, &str> = HashMap::new();
        body.insert("tracking_number", tracking_number);
        body.insert("carrier", carrier);

        // Build the request
        let request = self.request(Method::POST, "tracks", body, None);

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            StatusCode::CREATED => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        Ok(resp.json().await.unwrap_or_default())
    }

    /// Request the tracking status of a shipment by sending a GET request.
    /// FROM: https://goshippo.com/docs/reference#tracks-retrieve
    pub async fn get_tracking_status(
        &self,
        carrier: &str,
        tracking_number: &str,
    ) -> Result<TrackingStatus, APIError> {
        // Build the request
        let request = self.request(
            Method::GET,
            &format!("tracks/{}/{}", carrier, tracking_number),
            (),
            None,
        );

        let resp = self.client.execute(request).await.unwrap();
        match resp.status() {
            StatusCode::OK => (),
            s => {
                return Err(APIError {
                    status_code: s,
                    body: resp.text().await.unwrap(),
                })
            }
        };

        Ok(resp.json().await.unwrap_or_default())
    }
}

/// Error type returned by our library.
pub struct APIError {
    pub status_code: StatusCode,
    pub body: String,
}

impl fmt::Display for APIError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "APIError: status code -> {}, body -> {}",
            self.status_code.to_string(),
            self.body
        )
    }
}

impl fmt::Debug for APIError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "APIError: status code -> {}, body -> {}",
            self.status_code.to_string(),
            self.body
        )
    }
}

// This is important for other errors to wrap this one.
impl error::Error for APIError {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        // Generic error, underlying cause isn't tracked.
        None
    }
}

/// The data type for an API response.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct APIResponse {
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub next: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub previous: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty", alias = "results")]
    pub shipments: Vec<Shipment>,
}

/// The data type for an API response.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OrdersAPIResponse {
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub next: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub previous: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty", alias = "results")]
    pub orders: Vec<Order>,
}

/// The data type for an API response for carrier accounts.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CarrierAccountsAPIResponse {
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub next: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub previous: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty", alias = "results")]
    pub carrier_accounts: Vec<CarrierAccount>,
}

/// The data type for a transactions API response.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct TransactionsAPIResponse {
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub next: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub previous: String,

    #[serde(default, skip_serializing_if = "Vec::is_empty", alias = "results")]
    pub transactions: Vec<Transaction>,
}

/// The data type for a Shipment.
/// FROM: https://goshippo.com/docs/reference#shipments
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Shipment {
    /// "Waiting" shipments have been successfully submitted but not yet been
    /// processed. "Queued" shipments are currently being processed. "Success"
    /// shipments have been processed successfully, meaning that rate
    /// generation has concluded. "Error" does not occur currently and is
    /// reserved for future use.
    /// "WAITING" | "QUEUED" | "SUCCESS" | "ERROR"
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    /// Date and time of Shipment creation.
    pub object_created: DateTime<Utc>,
    /// Date and time of last Shipment update.
    pub object_updated: DateTime<Utc>,
    /// Unique identifier of the given Shipment object.
    pub object_id: String,
    /// Username of the user who created the Shipment object.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object_owner: String,
    /// Address object that should be used as sender Address.
    #[serde(default)]
    pub address_from: Address,
    /// Address object that should be used as recipient Address.
    #[serde(default)]
    pub address_to: Address,
    /// Address object where the shipment will be sent back to if it is not
    /// delivered (Only available for UPS, USPS, and Fedex shipments).
    /// If this field is not set, your shipments will be returned to the address_from.
    #[serde(default)]
    pub address_return: Address,
    /// Parcel objects to be shipped.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parcels: Vec<Parcel>,
    /// Date the shipment will be tendered to the carrier.
    /// Must be in the format "2014-01-18T00:35:03.463Z". Defaults to current
    /// date and time if no value is provided. Please note that some carriers
    /// require this value to be in the future, on a working day, or similar.
    pub shipment_date: DateTime<Utc>,
    /// Customs Declarations object for an international shipment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub customs_declaration: Option<CustomsDeclaration>,
    /// A string of up to 100 characters that can be filled with any additional
    /// information you want to attach to the object.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub metadata: String,
    /// An array with all available rates. If `async` has been set to `false`
    /// in the request, this will be populated with all available rates in the
    /// response. Otherwise rates will be created asynchronously and this array
    /// will initially be empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub rates: Vec<Rate>,
    /// Indicates whether the object has been created in test mode.
    #[serde(default)]
    pub test: bool,
}

/// The data type for a carrier account.
/// FROM: https://goshippo.com/docs/reference#carrier-accounts
#[derive(Clone, Debug, Default, JsonSchema, Serialize, Deserialize)]
pub struct CarrierAccount {
    /// Unique identifier of the given CarrierAccount object.
    pub object_id: String,
    /// Username of the user who created the CarrierAccount object.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object_owner: String,
    /// Name of the carrier. Please check the carrier accounts tutorial page
    /// for all supported carrier names.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub carrier: String,
    /// Unique identifier of the account.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub account_id: String,
    /// An array of additional parameters for the account, such as e.g. password or token.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub parameters: HashMap<String, String>,
    /// Determines whether the account is active. When creating a shipment, if
    /// no carrier_accounts are explicitly passed Shippo will query all carrier
    /// accounts that have this field set. By default, this is set to True.
    #[serde(default)]
    pub active: bool,
    /// Indicates whether the object has been created in test mode.
    #[serde(default)]
    pub test: bool,
}

/// The data type for an address.
/// FROM: https://goshippo.com/docs/reference#addresses
#[derive(Clone, Debug, Default, JsonSchema, Serialize, Deserialize)]
pub struct Address {
    /// Unique identifier of the given Address object. This ID is required to
    /// create a Shipment object.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object_id: String,
    /// Complete addresses contain all required values.
    /// Incomplete addresses have failed one or multiple validations.
    /// Incomplete Addresses are eligible for requesting rates but lack at
    /// least one required value for purchasing labels.
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_complete: bool,
    /// First and Last Name of the addressee
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    /// Company Name
    pub company: String,
    /// First street line, 35 character limit. Usually street number and street
    /// name (except for DHL Germany, see street_no).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub street1: String,
    /// Second street line, 35 character limit.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub street2: String,
    /// Name of a city. When creating a Quote Address, sending a city is
    /// optional but will yield more accurate Rates. Please bear in mind that
    /// city names may be ambiguous (there are 34 Springfields in the US).
    /// Pass in a state or a ZIP code (see below), if known, it will yield
    /// more accurate results.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub city: String,
    /// State/Province values are required for shipments from/to the US, AU,
    /// and CA. UPS requires province for some countries (i.e Ireland). To
    /// receive more accurate quotes, passing this field is recommended. Most
    /// carriers only accept two or three character state abbreviations.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub state: String,
    /// Postal code of an Address. When creating a Quote Addresses, sending a
    /// ZIP is optional but will yield more accurate Rates.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub zip: String,
    /// Example: 'US' or 'DE'. All accepted values can be found on the Official
    /// ISO Website. Sending a country is always required.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub country: String,
    /// Addresses containing a phone number allow carriers to call the recipient
    /// when delivering the Parcel. This increases the probability of delivery
    /// and helps to avoid accessorial charges after a Parcel has been shipped.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub phone: String,
    /// E-mail address of the contact person, RFC3696/5321-compliant.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub email: String,
    /// Indicates whether the object has been created in test mode.
    #[serde(
        default,
        skip_serializing_if = "is_false",
        deserialize_with = "deserialize_null_boolean::deserialize"
    )]
    pub test: bool,
    /// object that contains information regarding if an address had been validated or not. Also
    /// contains any messages generated during validation. Children keys are is_valid(boolean) and
    /// messages(array).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub validation_results: Option<ValidationResults>,
}

fn is_false(t: &bool) -> bool {
    !t
}

impl Address {
    pub fn formatted(&self) -> String {
        let street = format!("{}\n{}", self.street1, self.street2);
        let mut zip = self.zip.to_string();
        if self.country == "US" && zip.len() > 5 {
            zip.insert(5, '-');
        }
        format!(
            "{}\n{}, {} {} {}",
            street.trim(),
            self.city,
            self.state,
            zip,
            self.country
        )
        .trim()
        .trim_matches(',')
        .trim()
        .to_string()
    }
}

/// The data type for a parcel.
/// FROM: https://goshippo.com/docs/reference#parcels
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Parcel {
    /// A Parcel will only be valid when all required values have been sent and
    /// validated successfully.
    /// "VALID"
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object_state: String,
    /// Date and time of Parcel creation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_created: Option<DateTime<Utc>>,
    /// Date and time of last Parcel update. Since you cannot update Parcels
    /// after they were created, this time stamp reflects the time when the
    /// Parcel was changed by Shippo's systems for the last time, e.g.,
    /// during sorting the dimensions given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_updated: Option<DateTime<Utc>>,
    /// Unique identifier of the given Parcel object. This ID is required to
    /// create a Shipment object.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object_id: String,
    /// Username of the user who created the Parcel object.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object_owner: String,
    /// Length of the Parcel. Up to six digits in front and four digits after
    /// the decimal separator are accepted.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub length: String,
    /// Width of the Parcel. Up to six digits in front and four digits after
    /// the decimal separator are accepted.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub width: String,
    /// Height of the parcel. Up to six digits in front and four digits after
    /// the decimal separator are accepted.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub height: String,
    /// The unit used for length, width and height.
    /// "cm" | "in" | "ft" | "mm" | "m" | "yd"
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub distance_unit: String,
    /// Weight of the parcel. Up to six digits in front and four digits after
    /// the decimal separator are accepted.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub weight: String,
    /// The unit used for weight.
    /// "g" | "oz" | "lb" | "kg"
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mass_unit: String,
    /// A string of up to 100 characters that can be filled with any additional
    /// information you want to attach to the object.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub metadata: String,
    /// Indicates whether the object has been created in test mode.
    #[serde(default)]
    pub test: bool,
}

/// The data type for a rate.
/// A rate is an available service of a shipping provider for a given shipment,
/// typically including the price and transit time.
/// FROM: https://goshippo.com/docs/reference#rates
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Rate {
    /// Unique identifier of the given Rate object.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object_id: String,
    /// Date and time of Rate creation.
    pub object_created: DateTime<Utc>,
    /// Username of the user who created the rate object.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object_owner: String,
    /// An array containing specific attributes of this Rate in context of the
    /// entire shipment.
    /// Attributes can be assigned 'CHEAPEST', 'FASTEST', or 'BESTVALUE'.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attributes: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub shipment: String,
    /// Final Rate price, expressed in the currency used in the sender's country.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub amount: String,
    /// Currency used in the sender's country, refers to "amount". The official ISO 4217 currency
    /// codes are used, e.g. "USD" or "EUR".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub currency: String,
    /// Final Rate price, expressed in the currency used in the recipient's country.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub amount_local: String,
    /// Currency used in the recipient's country, refers to "amount_local". The official ISO 4217
    /// currency codes are used, e.g. "USD" or "EUR".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub currency_local: String,
    /// Carrier offering the rate, e.g., "FedEx" or "Deutsche Post DHL".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub provider: String,
    /// URL to the provider logo with max. dimensions of 75*75px.
    /// Please refer to the provider's Logo Usage Guidelines before using the logo.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub provider_image_75: String,
    /// URL to the provider logo with max. dimensions of 200*200px.
    /// Please refer to the provider's Logo Usage Guidelines before using the logo.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub provider_image_200: String,
    /// Contains details regarding the service level for the given rate.
    #[serde(default)]
    pub servicelevel: ServiceLevel,
    /// Estimated transit time (duration) in days of the Parcel at the given
    /// servicelevel. Please note that this is not binding, but only an average
    /// value as given by the provider. Shippo is not able to guarantee any
    /// transit times.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub estimated_days: Option<i64>,
    /// Further clarification of the transit times.
    /// Often, this includes notes that the transit time as given in "days"
    /// is only an average, not a guaranteed time.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub duration_terms: String,
    /// Object ID of the carrier account that has been used to retrieve the rate.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub carrier_account: String,
    /// Indicates whether the object has been created in test mode.
    #[serde(default)]
    pub test: bool,
}

/// The service level data type.
#[derive(Clone, Debug, Default, JsonSchema, Serialize, Deserialize)]
pub struct ServiceLevel {
    /// Name of the Rate's servicelevel, e.g. "International Priority" or
    /// "Standard Post".
    /// A servicelevel commonly defines the transit time of a Shipment
    /// (e.g., Express vs. Standard), along with other properties.
    /// These names vary depending on the provider.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub name: String,
    /// Token of the Rate's servicelevel, e.g. "usps_priority" or "fedex_ground".
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub token: String,
    /// Further clarification of the service.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub terms: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NewShipment {
    /// Address object that should be used as sender Address.
    #[serde(default)]
    pub address_from: Address,
    /// Address object that should be used as recipient Address.
    #[serde(default)]
    pub address_to: Address,
    /// Parcel objects to be shipped.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub parcels: Vec<Parcel>,
    /// Customs Declarations object for an international shipment.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub customs_declaration: Option<CustomsDeclaration>,
}

/// The data type for a pickup.
/// FROM: https://goshippo.com/docs/reference#pickups
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Pickup {
    /// Unique identifier of the given Pickup object.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object_id: String,
    /// Date and time of Pickup creation.
    pub object_created: DateTime<Utc>,
    /// Date and time of last Pickup update.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_updated: Option<DateTime<Utc>>,
    /// The object ID of your USPS or DHL Express carrier account.
    /// You can retrieve this from your Rate requests or our /carrier_accounts endpoint.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub carrier_account: String,
    /// Location where the parcel(s) will be picked up.
    #[serde(default)]
    pub location: Location,
    /// The transaction(s) object ID(s) for the parcel(s) that need to be picked up.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transactions: Vec<String>,
    /// The earliest that you requested your parcels to be ready for pickup.
    /// Expressed in the timezone specified in the response.
    pub requested_start_time: DateTime<Utc>,
    /// The latest that you requested your parcels to be available for pickup.
    /// Expressed in the timezone specified in the response.
    pub requested_end_time: DateTime<Utc>,
    /// The earliest that your parcels will be ready for pickup, confirmed by the carrier.
    /// Expressed in the timezone specified in the response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmed_start_time: Option<DateTime<Utc>>,
    /// The latest that your parcels will be available for pickup, confirmed by the carrier.
    /// Expressed in the timezone specified in the response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub confirmed_end_time: Option<DateTime<Utc>>,
    /// The latest time to cancel a pickup.
    /// Expressed in the timezone specified in the response.
    /// To cancel a pickup, you will need to contact the carrier directly.
    /// The ability to cancel a pickup through Shippo may be released in future iterations.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cancel_by_time: Option<DateTime<Utc>>,
    /// Indicates the status of the pickup.
    /// "PENDING" | "CONFIRMED" | "ERROR" | "CANCELLED"
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    /// Pickup's confirmation code returned by the carrier.
    /// To edit or cancel a pickup, you will need to contact USPS or DHL Express directly
    /// and provide your confirmation_code.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub confirmation_code: String,
    /// The pickup time windows will be in the time zone specified here, not UTC.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub timezone: String,
    /// An array containing strings of any messages generated during validation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub messages: Option<Vec<Message>>,
    /// A string of up to 100 characters that can be filled with any additional
    /// information you want to attach to the object.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub metadata: String,
    /// Indicates whether the object has been created in test mode.
    #[serde(default, skip_serializing_if = "is_false")]
    pub is_test: bool,
}

/// The location data type.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Location {
    /// Where your parcels will be available for pickup.
    /// "Security Deck" and "Shipping Dock" are only supported for DHL Express.
    /// "Front Door" | "Back Door" | "Side Door" | "Knock on Door" | "Ring Bell" | "Mail Room"
    /// "Office" | "Reception" | "In/At Mailbox" | "Security Deck" | "Shipping Dock" | "Other"
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub building_location_type: String,
    /// The type of building where the pickup is located.
    /// "apartment" | "building" | "department" | "floor" | "room" | "suite"
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub building_type: String,
    /// Pickup instructions for the courier.
    /// This is a mandatory field if the building_location_type is “Other”.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub instructions: String,
    /// The pickup address, which includes your name, company name, street address,
    /// city, state, zip code, country, phone number, and email address (strings).
    /// Special characters should not be included in any address element, especially
    /// name, company, and email.
    #[serde(default)]
    pub address: Address,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewPickup {
    #[serde(default)]
    pub carrier_account: String,
    pub location: Location,
    #[serde(default)]
    pub transactions: Vec<String>,
    pub requested_start_time: DateTime<Utc>,
    pub requested_end_time: DateTime<Utc>,
    #[serde(default)]
    pub is_test: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub metadata: String,
}

/// The data type for a transaction.
/// A transaction is the purchase of a shipping label from a shipping provider for a specific service.
/// FROM: https://goshippo.com/docs/reference#transactions
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Transaction {
    /// Unique identifier of the given Transaction object.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object_id: String,
    /// Date and time of Transaction creation.
    pub object_created: DateTime<Utc>,
    /// Date and time of last Transaction update.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object_updated: Option<DateTime<Utc>>,
    /// Username of the user who created the Transaction object.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object_owner: String,
    /// Indicates the status of the Transaction.
    /// "WAITING" | "QUEUED" | "SUCCESS" | "ERROR" | "REFUNDED" | "REFUNDPENDING" | "REFUNDREJECTED"
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub status: String,
    /// Indicates the validity of the Transaction object based on the given data,
    /// regardless of what the corresponding carrier returns.
    /// "VALID" | "INVALID"
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object_state: String,
    /// ID of the Rate object for which a Label has to be obtained.
    /// Please note that only rates that are not older than 7 days can be purchased
    /// in order to ensure up-to-date pricing.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub rate: String,
    /// A string of up to 100 characters that can be filled with any additional information you want to attach to the object.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub metadata: String,
    /// Specify the label file format for this label.
    /// If you don't specify this value, the API will default to your default file format that you can set on the settings page.
    /// "PNG" | "PNG_2.3x7.5" | "PDF" | "PDF_2.3x7.5" | "PDF_4x6" | "PDF_4x8" | "PDF_A4" | "PDF_A6"
    /// "ZPLII"
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label_file_type: String,
    /// The carrier-specific tracking number that can be used to track the Shipment.
    /// A value will only be returned if the Rate is for a trackable Shipment and if the Transactions has been processed successfully.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tracking_number: String,
    /// Indicates the high level status of the shipment: 'UNKNOWN', 'DELIVERED', 'TRANSIT', 'FAILURE', 'RETURNED'.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tracking_status: String,
    /// A link to track this item on the carrier-provided tracking website.
    /// A value will only be returned if tracking is available and the carrier provides such a service.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tracking_url_provider: String,
    /// The estimated time of arrival according to the carrier.
    #[serde(
        deserialize_with = "null_date_format::deserialize",
        skip_serializing_if = "Option::is_none"
    )]
    pub eta: Option<DateTime<Utc>>,
    /// A URL pointing directly to the label in the format you've set in your settings.
    /// A value will only be returned if the Transactions has been processed successfully.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label_url: String,
    /// A URL pointing to the commercial invoice as a 8.5x11 inch PDF file.
    /// A value will only be returned if the Transactions has been processed successfully and if the shipment is international.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub commercial_invoice_url: String,
    /// An array containing elements of the following schema:
    /// - "code" (string): an identifier for the corresponding message (not always available")
    /// - "message" (string): a publishable message containing further information.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<Message>,
    /// A URL pointing directly to the QR code in PNG format.
    /// A value will only be returned if requested using qr_code_requested flag and the carrier provides such an option.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub qr_code_url: String,
    /// Indicates whether the object has been created in test mode.
    #[serde(default)]
    pub test: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct NewTransaction {
    pub rate: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub metadata: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub label_file_type: String,
    #[serde(default)]
    pub r#async: bool,
}

#[derive(Clone, Debug, Default, JsonSchema, Serialize, Deserialize)]
pub struct Message {
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub source: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub code: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub text: String,
}

#[derive(Clone, Debug, Default, JsonSchema, Serialize, Deserialize)]
pub struct ValidationResults {
    #[serde(default)]
    pub is_valid: bool,
    /// An array containing elements of the following schema:
    /// - "code" (string): an identifier for the corresponding message (not always available")
    /// - "message" (string): a publishable message containing further information.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub messages: Vec<Message>,
}

/// The data type for a tracking status.
/// Tracking Status objects are used to track shipments.
/// FROM: https://goshippo.com/docs/reference#tracks
#[derive(Clone, Debug, Default, JsonSchema, Serialize, Deserialize)]
pub struct TrackingStatus {
    /// Name of the carrier of the shipment to track.
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub carrier: String,
    /// Tracking number to track.
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub tracking_number: String,
    /// The sender address with city, state, zip and country information.
    #[serde(default)]
    pub address_from: Option<Address>,
    /// The recipient address with city, state, zip and country information.
    #[serde(default)]
    pub address_to: Option<Address>,
    /// The object_id of the transaction associated with this tracking object.
    /// This field is visible only to the object owner of the transaction.
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub transaction: String,
    /// The estimated time of arrival according to the carrier, this might be
    /// updated by carriers during the life of the shipment.
    #[serde(
        deserialize_with = "null_date_format::deserialize",
        skip_serializing_if = "Option::is_none"
    )]
    pub eta: Option<DateTime<Utc>>,
    /// The estimated time of arrival according to the carrier at the time the
    /// shipment first entered the system.
    #[serde(
        deserialize_with = "null_date_format::deserialize",
        skip_serializing_if = "Option::is_none"
    )]
    pub original_eta: Option<DateTime<Utc>>,
    /// The service level of the shipment as token and full name.
    #[serde(default)]
    pub servicelevel: ServiceLevel,
    /// The latest tracking information of this shipment.
    pub tracking_status: Option<Status>,
    /// A list of tracking events, following the same structure as `tracking_status`.
    /// It contains a full history of all tracking statuses, starting with the earlier tracking event first.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tracking_history: Vec<Status>,
    /// A string of up to 100 characters that can be filled with any additional information you
    /// want to attach to the object.
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub metadata: String,
}

#[derive(Clone, Default, Debug, JsonSchema, Serialize, Deserialize)]
pub struct Status {
    /// Indicates the high level status of the shipment.
    /// 'UNKNOWN' | 'PRE_TRANSIT' | 'TRANSIT' | 'DELIVERED' | 'RETURNED' | 'FAILURE'
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub status: String,
    /// The human-readable description of the status.
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub status_details: String,
    /// Date and time when the carrier scanned this tracking event.
    /// This is displayed in UTC.
    #[serde(
        deserialize_with = "null_date_format::deserialize",
        skip_serializing_if = "Option::is_none"
    )]
    pub status_date: Option<DateTime<Utc>>,
    /// An object containing zip, city, state and country information of the tracking event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<TrackingLocation>,
}

#[derive(Clone, Debug, Default, JsonSchema, Serialize, Deserialize)]
pub struct TrackingLocation {
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub city: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub state: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub zip: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub country: String,
}

impl TrackingLocation {
    pub fn formatted(&self) -> String {
        let mut zip = self.zip.to_string();
        if self.country == "US" && zip.len() > 5 {
            zip.insert(5, '-');
        }
        format!("{}, {} {} {}", self.city, self.state, zip, self.country)
            .trim()
            .trim_matches(',')
            .trim()
            .to_string()
    }
}

/// A customs declaration object.
/// Customs declarations are relevant information, including one or multiple
/// customs items, you need to provide for customs clearance for your international shipments.
/// FROM: https://goshippo.com/docs/reference#customs-declarations
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CustomsDeclaration {
    /// Unique identifier of the given object.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object_id: String,
    /// Username of the user who created the object.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object_owner: String,
    /// Indicates the validity of the Customs Item.
    /// "VALID" | "INVALID"
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object_state: String,
    /// Exporter reference of an export shipment.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub exporter_reference: String,
    /// Importer reference of an import shipment.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub importer_reference: String,
    /// Type of goods of the shipment.
    /// 'DOCUMENTS' | 'GIFT' | 'SAMPLE' | 'MERCHANDISE' | 'HUMANITARIAN_DONATION'
    /// 'RETURN_MERCHANDISE' | 'OTHER'
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub contents_type: String,
    /// Explanation of the type of goods of the shipment.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub contents_explanation: String,
    /// Invoice reference of the shipment.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub invoice: String,
    /// License reference of the shipment.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub license: String,
    /// Certificate reference of the shipment.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub certificate: String,
    /// Additional notes to be included in the customs declaration.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub notes: String,
    /// EEL / PFC type of the shipment. For most shipments from the US to CA, 'NOEEI_30_36' is applicable; for most other shipments from the US, 'NOEEI_30_37_a' is applicable.
    /// 'NOEEI_30_37_a' | 'NOEEI_30_37_h' | 'NOEEI_30_37_f' | 'NOEEI_30_36' | 'AES_ITN'
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub eel_pfc: String,
    /// AES / ITN reference of the shipment.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub aes_itn: String,
    /// Indicates how the carrier should proceed in case the shipment can't be delivered.
    /// 'ABANDON' | 'RETURN'
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub non_delivery_option: String,
    /// Expresses that the certify_signer has provided all information of this customs declaration truthfully.
    #[serde(default)]
    pub certify: bool,
    /// Name of the person who created the customs declaration and is responsible for the validity of all information provided.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub certify_signer: String,
    /// Disclaimer for the shipment and customs information that have been provided.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub disclaimer: String,
    /// The incoterm reference of the shipment. FCA available for DHL Express and FedEx only.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub incoterm: String,
    /// B13A Option details are obtained by filing a B13A Canada Export Declaration via the Canadian Export Reporting System (CERS).
    /// 'FILED_ELECTRONICALLY' | SUMMARY_REPORTING' | 'NOT_REQUIRED'
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub b13a_filing_option: String,
    /// Represents: the Proof of Report (POR) Number when b13a_filing_option is FILED_ELECTRONICALLY;
    /// the Summary ID Number when b13a_filing_option is SUMMARY_REPORTING;
    /// or the Exemption Number when b13a_filing_option is NOT_REQUIRED.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub b13a_number: String,
    /// Distinct Parcel content items as Customs Items object_ids.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<String>,
    /// A string of up to 100 characters that can be filled with any additional information you want to attach to the object.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub metadata: String,
    /// Indicates whether the object has been created in test mode.
    #[serde(default)]
    pub test: bool,
}

/// An order object.
/// FROM: https://goshippo.com/docs/reference#orders
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Order {
    /// Unique identifier of the given object.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object_id: String,
    /// Username of the user who created the object.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object_owner: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub order_number: String,
    pub placed_at: DateTime<Utc>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub order_status: String,
    #[serde(default)]
    pub to_address: Address,
    #[serde(default)]
    pub from_address: Address,
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub shop_app: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub weight: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub weight_unit: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub transactions: Vec<Transaction>,
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub total_tax: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub total_price: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub subtotal_price: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub currency: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub shipping_method: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub shipping_cost: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub shipping_cost_currency: String,
    #[serde(
        default,
        deserialize_with = "deserialize_null_string::deserialize",
        skip_serializing_if = "String::is_empty"
    )]
    pub notes: String,
    #[serde(default)]
    pub test: bool,
}

/// A customs item object.
/// Customs items are distinct items in your international shipment parcel.
/// FROM: https://goshippo.com/docs/reference#customs-items
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CustomsItem {
    /// Unique identifier of the given object.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object_id: String,
    /// Username of the user who created the object.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object_owner: String,
    /// Indicates the validity of the Customs Item.
    /// "VALID" | "INVALID"
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub object_state: String,
    /// Text description of your item.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Quantity of this item in the shipment you send. Must be greater than 0.
    #[serde(default)]
    pub quantity: i64,
    /// Total weight of this item, i.e. quantity * weight per item.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub net_weight: String,
    /// The unit used for net_weight.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub mass_unit: String,
    /// Total value of this item, i.e. quantity * value per item.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value_amount: String,
    /// Currency used for value_amount. The official ISO 4217 currency codes are used, e.g. "USD" or "EUR".
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub value_currency: String,
    /// Country of origin of the item. Example: 'US' or 'DE'. All accepted values can be found on the Official ISO Website.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub origin_country: String,
    /// The tariff number of the item.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub tariff_number: String,
    /// SKU code of the item, which is required by some carriers.
    #[serde(
        default,
        skip_serializing_if = "String::is_empty",
        deserialize_with = "deserialize_null_string::deserialize"
    )]
    pub sku_code: String,
    /// Export Control Classification Number, required on some exports from the United States.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub eccn_ear99: String,
    /// A string of up to 100 characters that can be filled with any additional information you want to attach to the object.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub metadata: String,
    /// Indicates whether the object has been created in test mode.
    #[serde(default)]
    pub test: bool,
}

pub mod deserialize_null_string {
    use serde::{self, Deserialize, Deserializer};

    // The signature of a deserialize_with function must follow the pattern:
    //
    //    fn deserialize<'de, D>(D) -> Result<T, D::Error>
    //    where
    //        D: Deserializer<'de>
    //
    // although it may also be generic over the output types T.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<String, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer).unwrap_or_default();

        Ok(s)
    }
}

pub mod null_date_format {
    use chrono::{DateTime, TimeZone, Utc};
    use serde::{self, Deserialize, Deserializer};

    // The signature of a deserialize_with function must follow the pattern:
    //
    //    fn deserialize<'de, D>(D) -> Result<T, D::Error>
    //    where
    //        D: Deserializer<'de>
    //
    // although it may also be generic over the output types T.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<DateTime<Utc>>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer).unwrap_or_else(|_| "".to_string());
        if s.is_empty() {
            return Ok(None);
        }

        Ok(Some(Utc.datetime_from_str(&s, "%+").unwrap()))
    }
}

pub mod deserialize_null_boolean {
    use serde::{self, Deserializer};

    // The signature of a deserialize_with function must follow the pattern:
    //
    //    fn deserialize<'de, D>(D) -> Result<T, D::Error>
    //    where
    //        D: Deserializer<'de>
    //
    // although it may also be generic over the output types T.
    pub fn deserialize<'de, D>(deserializer: D) -> Result<bool, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = deserializer
            .deserialize_bool(crate::BoolVisitor)
            .unwrap_or_default();

        Ok(s)
    }
}

struct BoolVisitor;

impl<'de> Visitor<'de> for BoolVisitor {
    type Value = bool;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("a boolean")
    }

    fn visit_bool<E>(self, value: bool) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(value)
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match FromStr::from_str(value) {
            Ok(s) => Ok(s),
            Err(_) => Err(de::Error::invalid_value(
                de::Unexpected::Str(value),
                &"bool",
            )),
        }
    }

    fn visit_string<E>(self, value: String) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        match FromStr::from_str(&value) {
            Ok(s) => Ok(s),
            Err(_) => Err(de::Error::invalid_value(
                de::Unexpected::Str(&value),
                &"bool",
            )),
        }
    }
}
