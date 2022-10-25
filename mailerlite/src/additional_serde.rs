use chrono::NaiveDateTime;
use serde::{de, Serializer, Serialize, Deserializer, Deserialize};
use std::fmt;

use crate::FormattedDateTime;

impl Serialize for FormattedDateTime {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.0.format("%Y-%m-%d %H:%M:%S").to_string())
    }
}

struct FormattedDateTimeVisitor;

impl<'de> de::Visitor<'de> for FormattedDateTimeVisitor {
    type Value = FormattedDateTime;

    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("A string containing a date of the format Y-m-d H:M:S")
    }

    fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        NaiveDateTime::parse_from_str(&value, "%Y-%m-%d %H:%M:%S")
            .map(FormattedDateTime)
            .map_err(E::custom)
    }
}

impl<'de> Deserialize<'de> for FormattedDateTime {
    fn deserialize<D>(deserializer: D) -> Result<FormattedDateTime, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(FormattedDateTimeVisitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[test]
    fn test_serde_formatted_date_time() {
        let date = NaiveDateTime::from_timestamp(1666708534, 0);
        
        #[derive(Debug, Deserialize, Serialize, PartialEq)]
        struct Wrapper {
            inner: Option<FormattedDateTime>
        }

        let expected = Wrapper { inner: Some(FormattedDateTime(date)) };
        let serialized = serde_json::to_string(&expected).unwrap();
        let test = serde_json::from_str(&serialized).unwrap();

        assert_eq!(expected, test);
    }
}
