use std::env;
use std::error::Error;

use cloudflare::endpoints::{dns, zone};
use cloudflare::framework::{
    async_api::{ApiClient, Client},
    auth::Credentials,
    Environment, HttpApiClientConfig,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    let args: Vec<String> = env::args().collect();

    let domain = &args[1];
    let ip = &args[2];

    println!("Setting up Cloudflare A record from {} -> {}", ip, domain);
    // Create the Cloudflare client.
    let cf_creds = Credentials::UserAuthKey {
        email: env::var("CLOUDFLARE_EMAIL").unwrap(),
        key: env::var("CLOUDFLARE_TOKEN").unwrap(),
    };
    let api_client = Client::new(cf_creds, HttpApiClientConfig::default(), Environment::Production).unwrap();

    // We need the root of the domain not a subdomain.
    let domain_parts: Vec<&str> = domain.split('.').collect();
    let root_domain = format!("{}.{}", domain_parts[domain_parts.len() - 2], domain_parts[domain_parts.len() - 1]);

    // Get the zone ID for the domain.
    let zones = api_client
        .request(&zone::ListZones {
            params: zone::ListZonesParams {
                name: Some(root_domain.to_string()),
                ..Default::default()
            },
        })
        .await
        .unwrap()
        .result;

    // Our zone identifier should be the first record's ID.
    if zones.is_empty() {
        println!("we found no zones!");
        return Ok(());
    }
    let zone_identifier = &zones[0].id;

    // Check if we already have a TXT record and we need to update it.
    let dns_records = api_client
        .request(&dns::ListDnsRecords {
            zone_identifier,
            params: dns::ListDnsRecordsParams {
                name: Some(domain.to_string()),
                ..Default::default()
            },
        })
        .await
        .unwrap()
        .result;

    // If we have a dns record already, update it. If not, create it.
    let parsed: std::net::IpAddr = ip.parse().unwrap();
    if dns_records.is_empty() {
        // Create the DNS record.
        if parsed.is_ipv4() {
            let dns_record = api_client
                .request(&dns::CreateDnsRecord {
                    zone_identifier,
                    params: dns::CreateDnsRecordParams {
                        name: domain,
                        content: dns::DnsContent::A { content: ip.parse().unwrap() },
                        // This is the min.
                        ttl: Some(120),
                        proxied: None,
                        priority: None,
                    },
                })
                .await
                .unwrap()
                .result;
            println!("Created DNS record for ipv4: {:?}", dns_record);
        } else {
            let dns_record = api_client
                .request(&dns::CreateDnsRecord {
                    zone_identifier,
                    params: dns::CreateDnsRecordParams {
                        name: domain,
                        content: dns::DnsContent::AAAA { content: ip.parse().unwrap() },
                        // This is the min.
                        ttl: Some(120),
                        proxied: None,
                        priority: None,
                    },
                })
                .await
                .unwrap()
                .result;
            println!("Created DNS record for ipv6: {:?}", dns_record);
        }
    } else {
        // Create the DNS record.
        if parsed.is_ipv4() {
            // Update the DNS record.
            let dns_record = api_client
                .request(&dns::UpdateDnsRecord {
                    zone_identifier,
                    identifier: &dns_records[0].id,
                    params: dns::UpdateDnsRecordParams {
                        name: domain,
                        content: dns::DnsContent::A { content: ip.parse().unwrap() },
                        ttl: None,
                        proxied: None,
                    },
                })
                .await
                .unwrap()
                .result;

            println!("Updated DNS record ipv4: {:?}", dns_record);
        } else {
            // Update the DNS record.
            let dns_record = api_client
                .request(&dns::UpdateDnsRecord {
                    zone_identifier,
                    identifier: &dns_records[0].id,
                    params: dns::UpdateDnsRecordParams {
                        name: domain,
                        content: dns::DnsContent::AAAA { content: ip.parse().unwrap() },
                        ttl: None,
                        proxied: None,
                    },
                })
                .await
                .unwrap()
                .result;

            println!("Updated DNS record ipv6: {:?}", dns_record);
        }
    }

    Ok(())
}
