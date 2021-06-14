#![allow(clippy::never_loop)]
use regex::Regex;

/// This function returns a tracking number and a carrier.
/// The carrier is first followed by the tracking number.
pub fn parse_tracking_information(s: &str) -> (String, String) {
    let usps = parse_usps(s);
    if !usps.is_empty() {
        return ("USPS".to_string(), usps);
    }

    let ups = parse_ups(s);
    if !ups.is_empty() {
        return ("UPS".to_string(), ups);
    }

    let fedex = parse_fedex(s);
    if !fedex.is_empty() {
        return ("FedEx".to_string(), fedex);
    }

    ("".to_string(), "".to_string())
}

fn parse_ups(s: &str) -> String {
    let mut re = Regex::new(r"(?:1Z)[0-9A-Z]{16}").unwrap();
    for cap in re.captures_iter(s) {
        return (&cap[0]).to_string();
    }

    re = Regex::new(r"(?:T)+[0-9A-Z]{10}").unwrap();
    for cap in re.captures_iter(s) {
        return (&cap[0]).to_string();
    }

    re = Regex::new(r"[0-9]{26}").unwrap();
    for cap in re.captures_iter(s) {
        return (&cap[0]).to_string();
    }

    "".to_string()
}

fn parse_usps(s: &str) -> String {
    let mut re = Regex::new(r"(?:94|93|92|94|95)[0-9]{20}").unwrap();
    for cap in re.captures_iter(s) {
        return (&cap[0]).to_string();
    }

    re = Regex::new(r"(?:94|93|92|94|95)[0-9]{22}").unwrap();
    for cap in re.captures_iter(s) {
        return (&cap[0]).to_string();
    }

    re = Regex::new(r"(?:70|14|23|03)[0-9]{14}").unwrap();
    for cap in re.captures_iter(s) {
        return (&cap[0]).to_string();
    }

    re = Regex::new(r"(?:M0|82)[0-9]{8}").unwrap();
    for cap in re.captures_iter(s) {
        return (&cap[0]).to_string();
    }

    re = Regex::new(r"(?:[A-Z]{2})[0-9]{9}(?:[A-Z]{2})").unwrap();
    for cap in re.captures_iter(s) {
        return (&cap[0]).to_string();
    }

    "".to_string()
}

fn parse_fedex(s: &str) -> String {
    let mut re = Regex::new(r"[0-9]{20}").unwrap();
    for cap in re.captures_iter(s) {
        return (&cap[0]).to_string();
    }

    re = Regex::new(r"[0-9]{15}").unwrap();
    for cap in re.captures_iter(s) {
        return (&cap[0]).to_string();
    }

    re = Regex::new(r"[0-9]{12}").unwrap();
    for cap in re.captures_iter(s) {
        return (&cap[0]).to_string();
    }

    re = Regex::new(r"[0-9]{22}").unwrap();
    for cap in re.captures_iter(s) {
        return (&cap[0]).to_string();
    }

    "".to_string()
}

#[cfg(test)]
mod tests {
    use crate::tracking_numbers::parse_tracking_information;

    #[test]
    fn test_parse() {
        let example1 = r#"<tr>
<td width="40%"><font color="666666" size="2" face="Arial, Helvetica, sans-serif"><b>Tracking Number:</b> </font></td>
<a href="http://www.fedex.com/Tracking?action=track&amp;tracknumbers=784347694009" target="_blank">
<font color="00B2A9" size="2" face="Arial, Helvetica, sans-serif">784347694009</font>
</a>
</td>
</tr>"#;

        let (carrier, number) = parse_tracking_information(&example1);
        assert_eq!(carrier, "FedEx");
        assert_eq!(number, "784347694009");

        let example2 = r#"Your order from Mouser Electronics, Inc. is being processed by our
warehouse and will ship out on JUN 04, 2021.

You can track your order on the UPS website using their Online Tracking
Service
<http://wwwapps.ups.com/WebTracking/track?track=yes&trackNums=1Z7759450248880648>
.

Please note it may take up to 24 hours for tracking information to become
available online.

If you have any questions, please reply to this email or call our Customer
Service Team at 800-346-6873.

Thank you and we appreciate your business."#;
        let (carrier, number) = parse_tracking_information(&example2);
        assert_eq!(carrier, "UPS");
        assert_eq!(number, "1Z7759450248880648");
    }
}
