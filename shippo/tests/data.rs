/*!
 * Sample data from Shippo docs
 *
 * This crate exposes Shippo resources j
 */
use std::collections::HashMap;

use chrono::{offset::Utc, DateTime, NaiveDateTime};
use serde_json::json;

use shippo::{
    Address, CarrierAccount, CustomsItem, Location, NewPickup, NewShipment, NewTransaction, Order, Parcel, Pickup,
    Rate, ServiceLevel, Shipment, Status, TrackingLocation, TrackingStatus, Transaction, ValidationResults,
};

pub struct Response<D> {
    pub json_body: serde_json::Value,
    pub deserialized: D,
}

pub struct Request<B, D> {
    pub body: B,
    pub response: Response<D>,
}

/// FROM: https://goshippo.com/docs/reference#shipments-create
pub fn create_shipment() -> Request<NewShipment, Shipment> {
    Request {
        body: new_shipment(),
        response: Response {
            json_body: json!(
            {
               "object_created":"2014-07-17T00:04:06.163Z",
               "object_updated":"2014-07-17T00:04:06.163Z",
               "object_id":"89436997a794439ab47999701e60392e",
               "object_owner":"shippotle@goshippo.com",
               "status":"SUCCESS",
               "address_from":{
                  "object_id": "fdabf0abb93c4460b60aa596116872a7",
                  "validation_results": {},
                  "is_complete": true,
                  "name": "Mrs Hippo",
                  "company": "Shippo",
                  "street_no": "",
                  "street1": "1092 Indian Summer Ct",
                  "street2": "",
                  "street3": "",
                  "city": "San Jose",
                  "state": "CA",
                  "zip": "95122",
                  "country": "US",
                  "phone": "4159876543",
                  "email": "mrshippo@goshippo.com",
                  "is_residential": null
               },
               "address_to":{
                  "object_id": "0476d70c612a423f9509ba5f807569db",
                  "is_complete": true,
                  "validation_results": {},
                  "name": "Mr Hippo",
                  "company": "",
                  "street_no": "",
                  "street1": "965 Mission St #572",
                  "street2": "",
                  "street3": "",
                  "city": "San Francisco",
                  "state": "CA",
                  "zip": "94103",
                  "country": "US",
                  "phone": "4151234567",
                  "email": "mrhippo@goshippo.com",
                  "is_residential": null
               },
              "address_return":{
                  "object_id": "fdabf0abb93c4460b60aa596116872a7",
                  "is_complete": true,
                  "validation_results": {},
                  "name": "Mrs Hippo",
                  "company": "Shippo",
                  "street_no": "",
                  "street1": "1092 Indian Summer Ct",
                  "street2": "",
                  "street3": "",
                  "city": "San Jose",
                  "state": "CA",
                  "zip": "95122",
                  "country": "US",
                  "phone": "4159876543",
                  "email": "mrshippo@goshippo.com",
                  "is_residential": null
               },
              "parcels":[{
                  "object_id": "7df2ecf8b4224763ab7c71fae7ec8274",
                  "length": "10",
                  "width": "15",
                  "height": "10",
                  "distance_unit": "in",
                  "weight": "1",
                  "mass_unit": "lb"
               }],
               "shipment_date":"2014-07-17T00:04:06Z",
               "extra":{
                  "insurance": {
                    "amount": "",
                    "currency": ""
                  },
                  "reference_1":"",
                  "reference_2":""
               },
               "alternate_address_to": null,
               "customs_declaration":null,
               "rates": [
                    {
                        "object_created": "2014-07-17T00:04:06.163Z",
                        "object_id": "545ab0a1a6ea4c9f9adb2512a57d6d8b",
                        "object_owner": "shippotle@goshippo.com",
                        "shipment": "89436997a794439ab47999701e60392e",
                        "attributes": [],
                        "amount": "5.50",
                        "currency": "USD",
                        "amount_local": "5.50",
                        "currency_local": "USD",
                        "provider": "USPS",
                        "provider_image_75": "https://cdn2.goshippo.com/providers/75/USPS.png",
                        "provider_image_200": "https://cdn2.goshippo.com/providers/200/USPS.png",
                        "servicelevel": {
                          "name": "Priority Mail",
                          "token":"usps_priority",
                          "terms": "",
                          "extended_token": "usps_priority",
                          "parent_servicelevel": null
                        },
                        "estimated_days": 2,
                        "arrives_by": null,
                        "duration_terms": "Delivery in 1 to 3 business days.",
                        "messages": [],
                        "carrier_account": "078870331023437cb917f5187429b093",
                        "test": true,
                        "zone": "20"
                    },
                ],
               "carrier_accounts": [],
               "messages":[],
               "metadata":"Customer ID 123456",
               "test": true
            }),
            deserialized: shipment(),
        },
    }
}

/// FROM: https://goshippo.com/docs/reference#shipments-retrieve
pub fn get_shipment() -> Response<Shipment> {
    Response {
        json_body: json!(
        {
              "object_created": "2014-07-17T00:04:06.163Z",
              "object_updated": "2014-07-17T00:04:06.163Z",
              "object_id": "89436997a794439ab47999701e60392e",
              "object_owner": "shippotle@goshippo.com",
              "status": "SUCCESS",
              "address_from": {
                "object_id": "fdabf0abb93c4460b60aa596116872a7",
                "validation_results": {},
                "is_complete": true,
                "name": "Mrs Hippo",
                "company": "Shippo",
                "street_no": "",
                "street1": "1092 Indian Summer Ct",
                "street2": "",
                "street3": "",
                "city": "San Jose",
                "state": "CA",
                "zip": "95122",
                "country": "US",
                "phone": "4159876543",
                "email": "mrshippo@goshippo.com",
                "is_residential": null
              },
              "address_to": {
                "object_id": "0476d70c612a423f9509ba5f807569db",
                "is_complete": true,
                "validation_results": {},
                "name": "Mr Hippo",
                "company": "",
                "street_no": "",
                "street1": "965 Mission St #572",
                "street2": "",
                "street3": "",
                "city": "San Francisco",
                "state": "CA",
                "zip": "94103",
                "country": "US",
                "phone": "4151234567",
                "email": "mrhippo@goshippo.com",
                "is_residential": null
              },
              "parcels": [
                {
                  "object_id": "7df2ecf8b4224763ab7c71fae7ec8274",
                  "length": "10",
                  "width": "15",
                  "height": "10",
                  "distance_unit": "in",
                  "weight": "1",
                  "mass_unit": "lb"
                }
              ],
              "shipment_date": "2014-07-17T00:04:06Z",
              "address_return": {
                "object_id": "fdabf0abb93c4460b60aa596116872a7",
                "is_complete": true,
                "validation_results": {},
                "name": "Mrs Hippo",
                "company": "Shippo",
                "street_no": "",
                "street1": "1092 Indian Summer Ct",
                "street2": "",
                "street3": "",
                "city": "San Jose",
                "state": "CA",
                "zip": "95122",
                "country": "US",
                "phone": "4159876543",
                "email": "mrshippo@goshippo.com",
                "is_residential": null
              },
              "extra": {
                "insurance": {
                  "amount": "",
                  "currency": ""
                },
                "reference_1": "",
                "reference_2": ""
              },
              "alternate_address_to": null,
              "customs_declaration": null,
              "rates": [
                {
                  "object_created": "2014-07-17T00:04:06.163Z",
                  "object_id": "545ab0a1a6ea4c9f9adb2512a57d6d8b",
                  "object_owner": "shippotle@goshippo.com",
                  "shipment": "89436997a794439ab47999701e60392e",
                  "attributes": [],
                  "amount": "5.50",
                  "currency": "USD",
                  "amount_local": "5.50",
                  "currency_local": "USD",
                  "provider": "USPS",
                  "provider_image_75": "https://cdn2.goshippo.com/providers/75/USPS.png",
                  "provider_image_200": "https://cdn2.goshippo.com/providers/200/USPS.png",
                  "servicelevel": {
                    "name": "Priority Mail",
                    "token": "usps_priority",
                    "terms": "",
                    "extended_token": "usps_priority",
                    "parent_servicelevel": null
                  },
                  "estimated_days": 2,
                  "arrives_by": null,
                  "duration_terms": "Delivery in 1 to 3 business days.",
                  "messages": [],
                  "carrier_account": "078870331023437cb917f5187429b093",
                  "test": true,
                  "zone": "20"
                }
              ],
              "carrier_accounts": [],
              "messages": [
                {
                  "source": "UPS",
                  "code": "",
                  "text": "RatedShipmentWarning: Ship To Address Classification is changed from Commercial to Residential"
                }
              ],
              "metadata": "Customer ID 123456",
              "test": true
            }),
        deserialized: shipment(),
    }
}

/// FROM: https://goshippo.com/docs/reference#shipments-list
pub fn list_shipments() -> Response<Vec<Shipment>> {
    Response {
        json_body: json!(
        {
          "next": "https://api.goshippo.com/shipments/?page=2",
          "previous": null,
          "results": [
            {
              "object_created": "2014-07-17T00:04:06.163Z",
              "object_updated": "2014-07-17T00:04:06.163Z",
              "object_id": "89436997a794439ab47999701e60392e",
              "object_owner": "shippotle@goshippo.com",
              "status": "SUCCESS",
              "address_from": {
                "object_id": "fdabf0abb93c4460b60aa596116872a7",
                "validation_results": {},
                "is_complete": true,
                "name": "Mrs Hippo",
                "company": "Shippo",
                "street_no": "",
                "street1": "1092 Indian Summer Ct",
                "street2": "",
                "street3": "",
                "city": "San Jose",
                "state": "CA",
                "zip": "95122",
                "country": "US",
                "phone": "4159876543",
                "email": "mrshippo@goshippo.com",
                "is_residential": null
              },
              "address_to": {
                "object_id": "0476d70c612a423f9509ba5f807569db",
                "is_complete": true,
                "validation_results": {},
                "name": "Mr Hippo",
                "company": "",
                "street_no": "",
                "street1": "965 Mission St #572",
                "street2": "",
                "street3": "",
                "city": "San Francisco",
                "state": "CA",
                "zip": "94103",
                "country": "US",
                "phone": "4151234567",
                "email": "mrhippo@goshippo.com",
                "is_residential": null
              },
              "parcels": [
                {
                  "object_id": "7df2ecf8b4224763ab7c71fae7ec8274",
                  "length": "10",
                  "width": "15",
                  "height": "10",
                  "distance_unit": "in",
                  "weight": "1",
                  "mass_unit": "lb"
                }
              ],
              "shipment_date": "2014-07-17T00:04:06Z",
              "address_return": {
                "object_id": "fdabf0abb93c4460b60aa596116872a7",
                "is_complete": true,
                "validation_results": {},
                "name": "Mrs Hippo",
                "company": "Shippo",
                "street_no": "",
                "street1": "1092 Indian Summer Ct",
                "street2": "",
                "street3": "",
                "city": "San Jose",
                "state": "CA",
                "zip": "95122",
                "country": "US",
                "phone": "4159876543",
                "email": "mrshippo@goshippo.com",
                "is_residential": null
              },
              "extra": {
                "insurance": {
                  "amount": "",
                  "currency": ""
                },
                "reference_1": "",
                "reference_2": ""
              },
              "alternate_address_to": null,
              "customs_declaration": null,
              "rates": [
                {
                  "object_created": "2014-07-17T00:04:06.163Z",
                  "object_id": "545ab0a1a6ea4c9f9adb2512a57d6d8b",
                  "object_owner": "shippotle@goshippo.com",
                  "shipment": "89436997a794439ab47999701e60392e",
                  "attributes": [],
                  "amount": "5.50",
                  "currency": "USD",
                  "amount_local": "5.50",
                  "currency_local": "USD",
                  "provider": "USPS",
                  "provider_image_75": "https://cdn2.goshippo.com/providers/75/USPS.png",
                  "provider_image_200": "https://cdn2.goshippo.com/providers/200/USPS.png",
                  "servicelevel": {
                    "name": "Priority Mail",
                    "token": "usps_priority",
                    "terms": "",
                    "extended_token": "usps_priority",
                    "parent_servicelevel": null
                  },
                  "estimated_days": 2,
                  "arrives_by": null,
                  "duration_terms": "Delivery in 1 to 3 business days.",
                  "messages": [],
                  "carrier_account": "078870331023437cb917f5187429b093",
                  "test": true,
                  "zone": "20"
                }
              ],
              "carrier_accounts": [],
              "messages": [
                {
                  "source": "UPS",
                  "code": "",
                  "text": "RatedShipmentWarning: Ship To Address Classification is changed from Commercial to Residential"
                }
              ],
              "metadata": "Customer ID 123456",
              "test": true
            }
          ]
        }),
        deserialized: vec![shipment()],
    }
}

/// FROM: https://goshippo.com/docs/reference#rates-retrieve
pub fn get_rate() -> Response<Rate> {
    Response {
        json_body: json!(
        {
           "object_created":"2014-07-17T00:04:06.163Z",
           "object_id":"545ab0a1a6ea4c9f9adb2512a57d6d8b",
           "object_owner":"shippotle@goshippo.com",
           "shipment":"89436997a794439ab47999701e60392e",
           "attributes":[],
           "amount":"5.50",
           "currency":"USD",
           "amount_local":"5.50",
           "currency_local":"USD",
           "provider":"USPS",
           "provider_image_75":"https://cdn2.goshippo.com/providers/75/USPS.png",
           "provider_image_200":"https://cdn2.goshippo.com/providers/200/USPS.png",
           "servicelevel": {
              "name":"Priority Mail",
              "token": "usps_priority",
              "terms": "",
              "extended_token": "usps_priority",
              "parent_servicelevel": null
           },
           "estimated_days":2,
           "arrives_by": null,
           "duration_terms":"Delivery in 1 to 3 business days.",
           "carrier_account":"078870331023437cb917f5187429b093",
           "zone":"1",
           "messages":[],
           "test": true
        }),
        deserialized: rate(),
    }
}

/// FROM: https://goshippo.com/docs/reference#transactions-retrieve
pub fn get_shipping_label() -> Response<Transaction> {
    Response {
        json_body: json!(
        {
           "object_state":"VALID",
           "status":"SUCCESS",
           "object_created":"2014-07-25T02:09:34.422Z",
           "object_updated":"2014-07-25T02:09:34.513Z",
           "object_id":"ef8808606f4241ee848aa5990a09933c",
           "object_owner":"shippotle@goshippo.com",
           "test":true,
           "rate":"ee81fab0372e419ab52245c8952ccaeb",
           "tracking_number":"",
           "tracking_status":"UNKNOWN",
           "tracking_history": [],
           "tracking_url_provider":"",
           "eta":"",
           "label_url":"",
           "commercial_invoice_url": "",
           "messages":[

           ],
           "order": null,
           "metadata":"",
           "parcel": null,
           "billing": {
               "payments": []
           },
           "qr_code_url":""
        }),
        deserialized: transaction(),
    }
}

/// FROM: https://goshippo.com/docs/reference#pickups-create
pub fn create_pickup() -> Request<NewPickup, Pickup> {
    Request {
        body: new_pickup(),
        response: Response {
            json_body: json!(
            {
                "object_created": "2020-05-08T17:09:48.028Z",
                "object_updated": "2020-05-08T17:09:48.884Z",
                "object_id": "e0cefba8a75f401e893db1eb09075efb",
                "carrier_account": "6c51273296864869829b96a80fb13ea1",
                "location": {
                    "instructions": "Behind screen door",
                    "building_location_type": "Other",
                    "building_type": "suite",
                    "address": {
                        "object_id": "fdabf0abb93c4460b60aa596116872a7",
                        "validation_results": {},
                        "is_complete": true,
                        "name": "Mrs Hippo",
                        "company": "Shippo",
                        "street_no": "",
                        "street1": "1092 Indian Summer Ct",
                        "street2": "",
                        "street3": "",
                        "city": "San Jose",
                        "state": "CA",
                        "zip": "95122",
                        "country": "US",
                        "phone": "4159876543",
                        "email": "mrshippo@goshippo.com",
                        "is_residential": null
                    }
                },
                "transactions": [
                    "7439c279b374494c9a80ca24f59e6fc5"
                ],
                "requested_start_time": "2020-05-12T19:00:00Z",
                "requested_end_time": "2020-05-12T23:00:00Z",
                "confirmed_start_time": "2020-05-09T12:00:00Z",
                "confirmed_end_time": "2020-05-09T23:59:59.999Z",
                "cancel_by_time": "2020-05-09T08:00:00Z",
                "status": "CONFIRMED",
                "confirmation_code": "WTC310058750",
                "timezone": "US/Pacific",
                "messages": null,
                "metadata": "Customer ID 123456",
                "is_test": false
            }),
            deserialized: pickup(),
        },
    }
}

/// FROM: https://goshippo.com/docs/reference#customs-items-create
pub fn create_customs_item() -> Request<CustomsItem, CustomsItem> {
    Request {
        body: customs_item(),
        response: Response {
            json_body: json!(
            {
               "object_created":"2014-07-17T00:49:20.631Z",
               "object_updated":"2014-07-17T00:49:20.631Z",
               "object_id":"55358464c7b740aca199b395536981bd",
               "object_owner":"shippotle@goshippo.com",
               "object_state":"VALID",
               "description":"T-Shirt",
               "quantity":2,
               "net_weight":"400",
               "mass_unit":"g",
               "value_amount":"20",
               "value_currency":"USD",
               "origin_country":"US",
               "tariff_number":"",
               "hs_code": null,
               "sku_code": null,
               "eccn_ear99":"",
               "metadata":"Order ID '123123'",
               "test": true
            }),
            deserialized: customs_item(),
        },
    }
}

/// FROM: https://goshippo.com/docs/reference#customs-items-create
pub fn create_shipping_label_from_rate() -> Request<NewTransaction, Transaction> {
    Request {
        body: new_transaction(),
        response: Response {
            json_body: json!(
                {
                   "object_state":"VALID",
                   "status":"SUCCESS",
                   "object_created":"2014-07-25T02:09:34.422Z",
                   "object_updated":"2014-07-25T02:09:34.513Z",
                   "object_id":"ef8808606f4241ee848aa5990a09933c",
                   "object_owner":"shippotle@goshippo.com",
                   "test":true,
                   "rate":"ee81fab0372e419ab52245c8952ccaeb",
                   "tracking_number":"",
                   "tracking_status":"UNKNOWN",
                   "tracking_history": [],
                   "tracking_url_provider":"",
                   "eta":"",
                   "label_url":"",
                   "commercial_invoice_url": "",
                   "messages":[

                   ],
                   "order": null,
                   "metadata":"",
                   "parcel": null,
                   "billing": {
                       "payments": []
                   },
                   "qr_code_url":""
                }
            ),
            deserialized: transaction(),
        },
    }
}

/// FROM: https://goshippo.com/docs/reference#transactions-list
pub fn list_shipping_labels() -> Response<Vec<Transaction>> {
    Response {
        json_body: json!(
        {
           "next":null,
           "previous":null,
           "results":[
            {
               "object_state":"VALID",
               "status":"SUCCESS",
               "object_created":"2014-07-25T02:09:34.422Z",
               "object_updated":"2014-07-25T02:09:34.513Z",
               "object_id":"ef8808606f4241ee848aa5990a09933c",
               "object_owner":"shippotle@goshippo.com",
               "test":true,
               "rate":"ee81fab0372e419ab52245c8952ccaeb",
               "tracking_number":"",
               "tracking_status":"UNKNOWN",
               "tracking_history": [],
               "tracking_url_provider":"",
               "eta":"",
               "label_url":"",
               "commercial_invoice_url": "",
               "messages":[

               ],
               "order": null,
               "metadata":"",
               "parcel": null,
               "billing": {
                   "payments": []
               },
               "qr_code_url":""
            }
        ]}),
        deserialized: vec![transaction()],
    }
}

/// FROM: https://goshippo.com/docs/reference#tracks-create
pub fn register_tracking_webhook() -> Request<(String, String), TrackingStatus> {
    Request {
        body: (String::from("usps"), String::from("9205590164917312751089")),
        response: Response {
            json_body: json!(
            {
              "carrier": "usps",
              "tracking_number": "9205590164917312751089",
              "address_from": {
                "city": "Las Vegas",
                "state": "NV",
                "zip": "89101",
                "country": "US"
              },
              "address_to": {
                "city": "Spotsylvania",
                "state": "VA",
                "zip": "22551",
                "country": "US"
              },
              "transaction": "1275c67d754f45bf9d6e4d7a3e205314",
              "original_eta": "2016-07-23T00:00:00Z",
              "eta": "2016-07-23T00:00:00Z",
              "servicelevel": {
                "token": "usps_priority",
                "name": "Priority Mail"
              },
              "metadata": null,
              "tracking_status": {
                "object_created": "2016-07-23T20:35:26.129Z",
                "object_updated": "2016-07-23T20:35:26.129Z",
                "object_id": "ce48ff3d52a34e91b77aa98370182624",
                "status": "DELIVERED",
                "status_details": "Your shipment has been delivered at the destination mailbox.",
                "status_date": "2016-07-23T13:03:00Z",
                "location": {
                  "city": "Spotsylvania",
                  "state": "VA",
                  "zip": "22551",
                  "country": "US"
                }
              },
              "tracking_history": [
                {
                  "object_created": "2016-07-22T14:36:50.943Z",
                  "object_id": "265c7a7c23354da5b87b2bf52656c625",
                  "status": "TRANSIT",
                  "status_details": "Your shipment has been accepted.",
                  "status_date": "2016-07-21T15:33:00Z",
                  "location": {
                    "city": "Las Vegas",
                    "state": "NV",
                    "zip": "89101",
                    "country": "US"
                  }
                },
                {
                  "object_created": "2016-07-23T20:35:26.129Z",
                  "object_id": "aab1d7c0559d43ccbba4ff8603089e56",
                  "status": "DELIVERED",
                  "status_details": "Your shipment has been delivered at the destination mailbox.",
                  "status_date": "2016-07-23T13:03:00Z",
                  "location": {
                    "city": "Spotsylvania",
                    "state": "VA",
                    "zip": "22551",
                    "country": "US"
                  }
                }
              ],
              "messages": []
            }),
            deserialized: tracking_status(),
        },
    }
}

/// FROM: https://goshippo.com/docs/reference#tracks-retrieve
pub fn get_tracking_status() -> Response<TrackingStatus> {
    Response {
        json_body: json!(
        {
          "carrier": "usps",
          "tracking_number": "9205590164917312751089",
          "address_from": {
            "city": "Las Vegas",
            "state": "NV",
            "zip": "89101",
            "country": "US"
          },
          "address_to": {
            "city": "Spotsylvania",
            "state": "VA",
            "zip": "22551",
            "country": "US"
          },
          "transaction": "1275c67d754f45bf9d6e4d7a3e205314",
          "eta": "2016-07-23T00:00:00Z",
          "original_eta": "2016-07-23T00:00:00Z",
          "servicelevel": {
            "token": "usps_priority",
            "name": "Priority Mail"
          },
          "metadata": null,
          "tracking_status": {
            "object_created": "2016-07-23T20:35:26.129Z",
            "object_updated": "2016-07-23T20:35:26.129Z",
            "object_id": "ce48ff3d52a34e91b77aa98370182624",
            "status": "DELIVERED",
            "status_details": "Your shipment has been delivered at the destination mailbox.",
            "status_date": "2016-07-23T13:03:00Z",
            "location": {
              "city": "Spotsylvania",
              "state": "VA",
              "zip": "22551",
              "country": "US"
            }
          },
          "tracking_history": [
            {
              "object_created": "2016-07-22T14:36:50.943Z",
              "object_id": "265c7a7c23354da5b87b2bf52656c625",
              "status": "TRANSIT",
              "status_details": "Your shipment has been accepted.",
              "status_date": "2016-07-21T15:33:00Z",
              "location": {
                "city": "Las Vegas",
                "state": "NV",
                "zip": "89101",
                "country": "US"
              }
            },
            {
              "object_created": "2016-07-23T20:35:26.129Z",
              "object_id": "aab1d7c0559d43ccbba4ff8603089e56",
              "status": "DELIVERED",
              "status_details": "Your shipment has been delivered at the destination mailbox.",
              "status_date": "2016-07-23T13:03:00Z",
              "location": {
                "city": "Spotsylvania",
                "state": "VA",
                "zip": "22551",
                "country": "US"
              }
            }
          ],
          "messages": []
        }
        ),
        deserialized: tracking_status(),
    }
}

/// FROM: https://goshippo.com/docs/reference#orders-list
pub fn list_orders<S>(next: S) -> Response<Vec<Order>>
where
    S: Into<String>,
{
    Response {
        json_body: json!(
        {
           "next": next.into(),
           "previous": null,
           "results": [
              {
                 "object_id": "4f2bc588e4e5446cb3f9fdb7cd5e190b",
                 "object_owner": "shippotle@goshippo.com",
                 "order_number": "#1068",
                 "order_status": "PAID",
                 "placed_at": "2016-09-23T01:28:12Z",
                 "to_address": {
                    "object_state": "VALID",
                    "object_purpose": "PURCHASE",
                    "object_source": "FULLY_ENTERED",
                    "object_created": "2016-09-23T01:38:56Z",
                    "object_updated": "2016-09-23T01:38:56Z",
                    "object_id": "fdabf0abb93c4460b60aa596116872a7",
                    "object_owner": "shippotle@goshippo.com",
                    "is_complete": true,
                    "validation_results": {},
                    "name": "Mrs Hippo",
                    "company": "Shippo",
                    "street_no": "",
                    "street1": "1092 Indian Summer Ct",
                    "street2": "",
                    "street3": "",
                    "city": "San Jose",
                    "state": "CA",
                    "zip": "95122",
                    "country": "US",
                    "longitude": null,
                    "latitude": null,
                    "phone": "4159876543",
                    "email": "mrshippo@goshippo.com",
                    "is_residential": null,
                    "ip": null,
                    "messages": [],
                    "metadata": ""
                 },
                 "from_address": {
                    "object_state": "VALID",
                    "object_purpose": "PURCHASE",
                    "object_source": "FULLY_ENTERED",
                    "object_created": "2016-09-23T01:38:56Z",
                    "object_updated": "2016-09-23T01:38:56Z",
                    "object_id": "0476d70c612a423f9509ba5f807569db",
                    "object_owner": "shippotle@goshippo.com",
                    "is_complete": true,
                    "validation_results": {},
                    "name": "Mr Hippo",
                    "company": "",
                    "street_no": "",
                    "street1": "965 Mission St #572",
                    "street2": "",
                    "street3": "",
                    "city": "San Francisco",
                    "state": "CA",
                    "zip": "94103",
                    "country": "US",
                    "longitude": null,
                    "latitude": null,
                    "phone": "4151234567",
                    "email": "mrhippo@goshippo.com",
                    "is_residential": null,
                    "ip": null,
                    "messages": [],
                    "metadata": ""
                 },
                 "line_items": [
                    {
                       "object_id": "abf7d5675d744b6ea9fdb6f796b28f28",
                       "title": "Hippo Magazines",
                       "variant_title": "",
                       "sku": "HM-123",
                       "quantity": 1,
                       "total_price": "12.10",
                       "currency": "USD",
                       "weight": "0.40",
                       "weight_unit": "lb",
                       "manufacture_country": null,
                       "max_ship_time": null,
                       "max_delivery_time": null
                    }
                 ],
                 "items": [],
                 "hidden": false,
                 "shipping_cost": "12.83",
                 "shipping_cost_currency": "USD",
                 "shipping_method": "USPS First Class Package",
                 "shop_app": "Shippo",
                 "subtotal_price": "12.10",
                 "total_price": "24.93",
                 "total_tax": "0.00",
                 "currency": "USD",
                 "transactions": [],
                 "weight": "0.40",
                 "weight_unit": "lb",
                 "notes": null
              },
           ]
        }),
        deserialized: vec![order()],
    }
}

/// FROM: https://goshippo.com/docs/reference#carrier-accounts-list
pub fn list_carrier_accounts<S>(next: S) -> Response<Vec<CarrierAccount>>
where
    S: Into<String>,
{
    Response {
        json_body: json!(
        {
           "next": next.into(),
           "previous":null,
           "results":[
                 {
                    "object_id":"b741b99f95e841639b54272834bc478c",
                     "object_owner": "shippotle@goshippo.com",
                     "carrier": "fedex",
                     "carrier_name": "FedEx",
                     "carrier_images": {
                         "75": "https://shippo-static.s3.amazonaws.com/providers/75/FedEx.png",
                         "200": "https://shippo-static.s3.amazonaws.com/providers/200/FedEx.png"
                     },
                     "service_levels": [
                         {
                             "token": "fedex_same_day",
                             "name": "SameDay®",
                             "supports_return_labels": false
                         },
                     ],
                     "account_id": "account-id-123",
                     "parameters": {
                         "meter": "meter-123"
                     },
                     "test": false,
                     "active": true
                 },
           ]
        }),
        deserialized: vec![carrier_account()],
    }
}

/// FROM: https://goshippo.com/docs/reference#transactions
fn transaction() -> Transaction {
    Transaction {
        object_id: String::from("ef8808606f4241ee848aa5990a09933c"),
        object_created: Some(DateTime::<Utc>::from_utc(
            NaiveDateTime::parse_from_str("2014-07-25T02:09:34.422Z", "%Y-%m-%dT%H:%M:%S%.f%Z").unwrap(),
            Utc,
        )),
        object_updated: Some(DateTime::<Utc>::from_utc(
            NaiveDateTime::parse_from_str("2014-07-25T02:09:34.513Z", "%Y-%m-%dT%H:%M:%S%.f%Z").unwrap(),
            Utc,
        )),
        object_owner: String::from("shippotle@goshippo.com"),
        status: String::from("SUCCESS"),
        object_state: String::from("VALID"),
        rate: String::from("ee81fab0372e419ab52245c8952ccaeb"),
        metadata: String::from(""),
        label_file_type: String::from(""),
        tracking_number: String::from(""),
        tracking_status: String::from("UNKNOWN"),
        tracking_url_provider: String::from(""),
        eta: None,
        label_url: String::from(""),
        commercial_invoice_url: String::from(""),
        messages: vec![],
        qr_code_url: String::from(""),
        test: true,
    }
}

fn new_transaction() -> NewTransaction {
    NewTransaction {
        rate: String::from("cf6fea899f1848b494d9568e8266e076"),
        metadata: String::from(""),
        label_file_type: String::from("PDF"),
        r#async: false,
    }
}

/// FROM: https://goshippo.com/docs/reference#shipments
fn shipment() -> Shipment {
    Shipment {
        status: String::from("SUCCESS"),
        test: true,
        metadata: String::from("Customer ID 123456"),
        object_id: String::from("89436997a794439ab47999701e60392e"),
        object_owner: String::from("shippotle@goshippo.com"),
        object_created: DateTime::<Utc>::from_utc(
            NaiveDateTime::parse_from_str("2014-07-17T00:04:06.163Z", "%Y-%m-%dT%H:%M:%S%.f%Z").unwrap(),
            Utc,
        ),
        object_updated: DateTime::<Utc>::from_utc(
            NaiveDateTime::parse_from_str("2014-07-17T00:04:06.163Z", "%Y-%m-%dT%H:%M:%S%.f%Z").unwrap(),
            Utc,
        ),
        customs_declaration: None,
        shipment_date: DateTime::<Utc>::from_utc(
            NaiveDateTime::parse_from_str("2014-07-17T00:04:06Z", "%Y-%m-%dT%H:%M:%S%Z").unwrap(),
            Utc,
        ),
        parcels: vec![parcel()],
        rates: vec![rate()],
        address_to: mr_hippo_address(),
        address_return: mrs_hippo_address(),
        address_from: mrs_hippo_address(),
    }
}

/// FROM: https://goshippo.com/docs/reference#customs-items
fn customs_item() -> CustomsItem {
    CustomsItem {
        object_id: String::from("55358464c7b740aca199b395536981bd"),
        object_owner: String::from("shippotle@goshippo.com"),
        object_state: String::from("VALID"),
        description: String::from("T-Shirt"),
        quantity: 2,
        net_weight: String::from("400"),
        mass_unit: String::from("g"),
        value_amount: String::from("20"),
        value_currency: String::from("USD"),
        origin_country: String::from("US"),
        tariff_number: String::from(""),
        sku_code: String::from(""),
        eccn_ear99: String::from(""),
        metadata: String::from("Order ID '123123'"),
        test: true,
    }
}

/// FROM: https://goshippo.com/docs/reference#pickups
fn pickup() -> Pickup {
    Pickup {
        object_id: String::from("e0cefba8a75f401e893db1eb09075efb"),
        object_created: DateTime::<Utc>::from_utc(
            NaiveDateTime::parse_from_str("2020-05-08T17:09:48.028Z", "%Y-%m-%dT%H:%M:%S%.f%Z").unwrap(),
            Utc,
        ),
        object_updated: Some(DateTime::<Utc>::from_utc(
            NaiveDateTime::parse_from_str("2020-05-08T17:09:48.884Z", "%Y-%m-%dT%H:%M:%S%.f%Z").unwrap(),
            Utc,
        )),
        carrier_account: String::from("6c51273296864869829b96a80fb13ea1"),
        location: location(),
        transactions: vec![String::from("7439c279b374494c9a80ca24f59e6fc5")],
        requested_start_time: DateTime::<Utc>::from_utc(
            NaiveDateTime::parse_from_str("2020-05-12T19:00:00Z", "%Y-%m-%dT%H:%M:%S%Z").unwrap(),
            Utc,
        ),
        requested_end_time: DateTime::<Utc>::from_utc(
            NaiveDateTime::parse_from_str("2020-05-12T23:00:00Z", "%Y-%m-%dT%H:%M:%S%Z").unwrap(),
            Utc,
        ),
        confirmed_start_time: Some(DateTime::<Utc>::from_utc(
            NaiveDateTime::parse_from_str("2020-05-09T12:00:00Z", "%Y-%m-%dT%H:%M:%S%Z").unwrap(),
            Utc,
        )),
        confirmed_end_time: Some(DateTime::<Utc>::from_utc(
            NaiveDateTime::parse_from_str("2020-05-09T23:59:59.999Z", "%Y-%m-%dT%H:%M:%S%.f%Z").unwrap(),
            Utc,
        )),
        cancel_by_time: Some(DateTime::<Utc>::from_utc(
            NaiveDateTime::parse_from_str("2020-05-09T08:00:00Z", "%Y-%m-%dT%H:%M:%S%Z").unwrap(),
            Utc,
        )),
        status: String::from("CONFIRMED"),
        confirmation_code: String::from("WTC310058750"),
        timezone: String::from("US/Pacific"),
        messages: None,
        metadata: String::from("Customer ID 123456"),
        is_test: false,
    }
}

fn new_pickup() -> NewPickup {
    NewPickup {
        carrier_account: String::from("6c51273296864869829b96a80fb13ea1"),
        location: location(),
        transactions: vec![String::from("7439c279b374494c9a80ca24f59e6fc5")],
        requested_start_time: DateTime::<Utc>::from_utc(
            NaiveDateTime::parse_from_str("2020-05-12T12:00:00Z", "%Y-%m-%dT%H:%M:%S%Z").unwrap(),
            Utc,
        ),
        requested_end_time: DateTime::<Utc>::from_utc(
            NaiveDateTime::parse_from_str("2020-05-12T16:00:00Z", "%Y-%m-%dT%H:%M:%S%Z").unwrap(),
            Utc,
        ),
        is_test: false,
        metadata: String::from("Cusomter ID 123456"),
    }
}

fn location() -> Location {
    Location {
        building_location_type: String::from("Other"),
        building_type: String::from("suite"),
        instructions: String::from("Behind screen door"),
        address: mrs_hippo_address(),
    }
}

fn new_shipment() -> NewShipment {
    NewShipment {
        address_from: mrs_hippo_address(),
        address_to: mr_hippo_address(),
        parcels: vec![parcel()],
        customs_declaration: None,
    }
}

/// FROM: https://goshippo.com/docs/reference#addresses
fn mr_hippo_address() -> Address {
    Address {
        object_id: String::from("0476d70c612a423f9509ba5f807569db"),
        is_complete: true,
        name: String::from("Mr Hippo"),
        company: String::from(""),
        street1: String::from("965 Mission St #572"),
        street2: String::from(""),
        city: String::from("San Francisco"),
        state: String::from("CA"),
        zip: String::from("94103"),
        country: String::from("US"),
        phone: String::from("4151234567"),
        email: String::from("mrhippo@goshippo.com"),
        test: false,
        validation_results: Some(ValidationResults {
            is_valid: false,
            messages: vec![],
        }),
    }
}

/// FROM: https://goshippo.com/docs/reference#addresses
fn mrs_hippo_address() -> Address {
    Address {
        object_id: String::from("fdabf0abb93c4460b60aa596116872a7"),
        is_complete: true,
        name: String::from("Mrs Hippo"),
        company: String::from("Shippo"),
        street1: String::from("1092 Indian Summer Ct"),
        street2: String::from(""),
        city: String::from("San Jose"),
        state: String::from("CA"),
        zip: String::from("95122"),
        country: String::from("US"),
        phone: String::from("4159876543"),
        email: String::from("mrshippo@goshippo.com"),
        test: false,
        validation_results: Some(ValidationResults {
            is_valid: false,
            messages: vec![],
        }),
    }
}

/// FROM: https://goshippo.com/docs/reference#rates
fn rate() -> Rate {
    Rate {
        object_id: String::from("545ab0a1a6ea4c9f9adb2512a57d6d8b"),
        object_owner: String::from("shippotle@goshippo.com"),
        object_created: DateTime::<Utc>::from_utc(
            NaiveDateTime::parse_from_str("2014-07-17T00:04:06.163Z", "%Y-%m-%dT%H:%M:%S%.f%Z").unwrap(),
            Utc,
        ),
        attributes: vec![],
        shipment: String::from("89436997a794439ab47999701e60392e"),
        amount: String::from("5.50"),
        currency: String::from("USD"),
        amount_local: String::from("5.50"),
        currency_local: String::from("USD"),
        provider: String::from("USPS"),
        provider_image_75: String::from("https://cdn2.goshippo.com/providers/75/USPS.png"),
        provider_image_200: String::from("https://cdn2.goshippo.com/providers/200/USPS.png"),
        servicelevel: ServiceLevel {
            name: String::from("Priority Mail"),
            token: String::from("usps_priority"),
            terms: String::from(""),
        },
        carrier_account: String::from("078870331023437cb917f5187429b093"),
        duration_terms: String::from("Delivery in 1 to 3 business days."),
        estimated_days: Some(2),
        test: true,
    }
}

/// FROM: https://goshippo.com/docs/reference#parcels
fn parcel() -> Parcel {
    Parcel {
        object_state: String::from(""),
        object_created: None,
        object_updated: None,
        object_owner: String::from(""),
        object_id: String::from("7df2ecf8b4224763ab7c71fae7ec8274"),
        metadata: String::from(""),
        test: false,
        length: String::from("10"),
        width: String::from("15"),
        height: String::from("10"),
        distance_unit: String::from("in"),
        weight: String::from("1"),
        mass_unit: String::from("lb"),
    }
}

/// FROM: https://goshippo.com/docs/reference#tracks
fn tracking_status() -> TrackingStatus {
    TrackingStatus {
        carrier: String::from("usps"),
        tracking_number: String::from("9205590164917312751089"),
        address_from: Some(Address {
            object_id: String::from(""),
            is_complete: false,
            name: String::from(""),
            company: String::from(""),
            street1: String::from(""),
            street2: String::from(""),
            city: String::from("Las Vegas"),
            state: String::from("NV"),
            zip: String::from("89101"),
            country: String::from("US"),
            phone: String::from(""),
            email: String::from(""),
            test: false,
            validation_results: None,
        }),
        address_to: Some(Address {
            object_id: String::from(""),
            is_complete: false,
            name: String::from(""),
            company: String::from(""),
            street1: String::from(""),
            street2: String::from(""),
            city: String::from("Spotsylvania"),
            state: String::from("VA"),
            zip: String::from("22551"),
            country: String::from("US"),
            phone: String::from(""),
            email: String::from(""),
            test: false,
            validation_results: None,
        }),
        transaction: String::from("1275c67d754f45bf9d6e4d7a3e205314"),
        eta: Some(DateTime::<Utc>::from_utc(
            NaiveDateTime::parse_from_str("2016-07-23T00:00:00Z", "%Y-%m-%dT%H:%M:%S%Z").unwrap(),
            Utc,
        )),
        original_eta: Some(DateTime::<Utc>::from_utc(
            NaiveDateTime::parse_from_str("2016-07-23T00:00:00Z", "%Y-%m-%dT%H:%M:%S%Z").unwrap(),
            Utc,
        )),
        servicelevel: service_level(),
        tracking_status: Some(delivered_status()),
        tracking_history: vec![transit_status(), delivered_status()],
        metadata: String::from(""),
    }
}

fn service_level() -> ServiceLevel {
    ServiceLevel {
        name: String::from("Priority Mail"),
        token: String::from("usps_priority"),
        terms: String::from(""),
    }
}

fn transit_status() -> Status {
    Status {
        status: String::from("TRANSIT"),
        status_details: String::from("Your shipment has been accepted."),
        status_date: Some(DateTime::<Utc>::from_utc(
            NaiveDateTime::parse_from_str("2016-07-21T15:33:00Z", "%Y-%m-%dT%H:%M:%S%Z").unwrap(),
            Utc,
        )),
        location: Some(TrackingLocation {
            city: String::from("Las Vegas"),
            state: String::from("NV"),
            zip: String::from("89101"),
            country: String::from("US"),
        }),
    }
}

fn delivered_status() -> Status {
    Status {
        status: String::from("DELIVERED"),
        status_details: String::from("Your shipment has been delivered at the destination mailbox."),
        status_date: Some(DateTime::<Utc>::from_utc(
            NaiveDateTime::parse_from_str("2016-07-23T13:03:00Z", "%Y-%m-%dT%H:%M:%S%Z").unwrap(),
            Utc,
        )),
        location: Some(TrackingLocation {
            city: String::from("Spotsylvania"),
            state: String::from("VA"),
            zip: String::from("22551"),
            country: String::from("US"),
        }),
    }
}

/// FROM: https://goshippo.com/docs/reference#orders
fn order() -> Order {
    Order {
        object_id: String::from("4f2bc588e4e5446cb3f9fdb7cd5e190b"),
        object_owner: String::from("shippotle@goshippo.com"),
        order_number: String::from("#1068"),
        placed_at: DateTime::<Utc>::from_utc(
            NaiveDateTime::parse_from_str("2016-09-23T01:28:12Z", "%Y-%m-%dT%H:%M:%S%.f%Z").unwrap(),
            Utc,
        ),
        order_status: String::from("PAID"),
        to_address: mrs_hippo_address(),
        from_address: mr_hippo_address(),
        shop_app: String::from("Shippo"),
        weight: String::from("0.40"),
        weight_unit: String::from("lb"),
        transactions: vec![],
        total_tax: String::from("0.00"),
        total_price: String::from("24.93"),
        subtotal_price: String::from("12.10"),
        currency: String::from("USD"),
        shipping_method: String::from("USPS First Class Package"),
        shipping_cost: String::from("12.83"),
        shipping_cost_currency: String::from("USD"),
        notes: String::from(""),
        test: false,
    }
}

/// FROM: https://goshippo.com/docs/reference#carrier-accounts
fn carrier_account() -> CarrierAccount {
    let mut parameter_map = HashMap::new();
    parameter_map.insert(String::from("meter"), String::from("meter-123"));
    CarrierAccount {
        object_id: String::from("b741b99f95e841639b54272834bc478c"),
        object_owner: String::from("shippotle@goshippo.com"),
        carrier: String::from("fedex"),
        account_id: String::from("account-id-123"),
        parameters: parameter_map,
        active: true,
        test: false,
    }
}
