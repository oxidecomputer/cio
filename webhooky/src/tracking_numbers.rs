#![allow(clippy::never_loop)]
use regex::Regex;

/// This function returns a tracking number and a carrier.
/// The carrier is first followed by the tracking number.
pub fn parse_tracking_information(s: &str) -> (String, String) {
    if s.to_lowercase().contains("ups.com") {
        let ups = parse_ups(s);
        if !ups.is_empty() {
            return ("UPS".to_string(), ups);
        }
    }

    if s.to_lowercase().contains("fedex.com") {
        let fedex = parse_fedex(s);
        if !fedex.is_empty() {
            return ("FedEx".to_string(), fedex);
        }
    }

    if s.to_lowercase().contains("usps.com") || s.to_lowercase().contains("carrier: usps") {
        let usps = parse_usps(s);
        if !usps.is_empty() {
            return ("USPS".to_string(), usps);
        }
    }

    if s.to_lowercase()
        .contains("http://texasinstruments.narvar.com/tracking/texasinstruments/dhl")
    {
        let dhl = parse_dhl(s);
        if !dhl.is_empty() {
            return ("DHL".to_string(), dhl);
        }
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

fn parse_dhl(s: &str) -> String {
    let re = Regex::new(r"[0-9]{10}").unwrap();
    for cap in re.captures_iter(s) {
        return (&cap[0]).to_string();
    }

    "".to_string()
}

fn parse_fedex(s: &str) -> String {
    let mut re = Regex::new(r"tracknumbers=[0-9]{20}").unwrap();
    for cap in re.captures_iter(s) {
        return (&cap[0]).trim_start_matches("tracknumbers=").to_string();
    }

    re = Regex::new(r"tracknumbers=[0-9]{15}").unwrap();
    for cap in re.captures_iter(s) {
        return (&cap[0]).trim_start_matches("tracknumbers=").to_string();
    }

    re = Regex::new(r"tracknumbers=[0-9]{12}").unwrap();
    for cap in re.captures_iter(s) {
        return (&cap[0]).trim_start_matches("tracknumbers=").to_string();
    }

    re = Regex::new(r"tracknumbers=[0-9]{22}").unwrap();
    for cap in re.captures_iter(s) {
        return (&cap[0]).trim_start_matches("tracknumbers=").to_string();
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

        let (carrier, number) = parse_tracking_information(example1);
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
        let (carrier, number) = parse_tracking_information(example2);
        assert_eq!(carrier, "UPS");
        assert_eq!(number, "1Z7759450248880648");

        let example3 = r#"Parsed email from Kirstin Neira:
<div><br></div><div><br><div class="gmail_quote"><div dir="ltr" class="gmail_attr">---------- Forwarded message ---------<br>From: <strong class="gmail_sendername" dir="auto">order_ship via procurement</strong> <span dir="auto">&lt;;</span><br>Date: Fri, Aug 13, 2021 at 9:41 PM<br>Subject: (Ref Kate Hicks) Your Coilcraft order has been shipped<br>To:
      <tbody><tr>			<td valign="top" width="75%"><b><p class="m_-7733175561257369484margin"><font size="5">Receipt / Shipping Notification<br>
				</font></p></b><i><font size="2">Please print this receipt for your records</font></i><br><br>
				<p class="m_-7733175561257369484margin"><b><font color="FF0000">Tracking number:</font> 525685736518&amp;nbsp &amp;nbsp<a href="http://www.fedex.com/Tracking?tracknumbers=525685736518" target="_blank">Click here to track</a></b></p>
													<p class="m_-7733175561257369484margin"><b><font color="FF0000">Order confirmation number:</font> CO 2616698</b>   Your PO Kate Hicks<b><br></b></p>
				<p class="m_-7733175561257369484margin"><b><font color="FF0000">Order date:</font></b>
				Tuesday August 03, 2021
				</p></td>
			<td valign="top" width="25%">
				<p align="right"><img border="0" src="https://www.coilcraft.com/content/images/email/coilbox135.png" width="135" height="41"></p></td>
		</tr>		<tr>
			<td colspan="2"><p class="m_-7733175561257369484margin"><font color="FF0000"><br>
				</font>Thank you for your on-line order</p>
				<p class="m_-7733175561257369484margin"><font color="FF0000"><img border="0" src="https://www.coilcraft.com/content/images/email/boxred.png"></font> Your package was shipped on
				Wednesday August 04, 2021
				via FedEx Ground (1-4 day).<br></p>
				<p class="m_-7733175561257369484margin"><font color="FF0000"><img border="0" src="https://www.coilcraft.com/content/images/email/boxred.png"></font> Backordered items should ship on the date shown below and will not be billed until then.<br></p>
				<p class="m_-7733175561257369484margin"><font color="F0000"><img border="0" src="https://www.coilcraft.com/content/images/email/boxred.png"></font> All shipping costs are estimated costs.<br></p>
				<p class="m_-7733175561257369484margin"><font color="F0000"><img border="0" src="https://www.coilcraft.com/content/images/email/boxred.png"></font> For help, contact <b>Barry Booker</b> at <b>847-516-7301</b> <a href="mailto:bbooker@coilcraft.com" target="_blank">bbooker@coilcraft.com</a></p>
		</td></tr>
	<table class="m_-7733175561257369484orderd1" border="1" width="675" cellspacing="0">
	<table class="m_-7733175561257369484orderd1" border="1" width="675" cellspacing="0">
</div></div>"#;
        let (carrier, number) = parse_tracking_information(example3);
        assert_eq!(carrier, "FedEx");
        assert_eq!(number, "525685736518");
    }
}
