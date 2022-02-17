use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub struct StatesMap {
    states: HashMap<String, String>,
}
impl StatesMap {
    #[tracing::instrument]
    fn populate(&mut self) {
        let mut map = HashMap::new();
        map.insert("AL".to_string(), "Alabama".to_string());
        map.insert("AK".to_string(), "Alaska".to_string());
        map.insert("AS".to_string(), "American Samoa".to_string());
        map.insert("AZ".to_string(), "Arizona".to_string());
        map.insert("AR".to_string(), "Arkansas".to_string());
        map.insert("CA".to_string(), "California".to_string());
        map.insert("CO".to_string(), "Colorado".to_string());
        map.insert("CT".to_string(), "Connecticut".to_string());
        map.insert("DE".to_string(), "Delaware".to_string());
        map.insert("DC".to_string(), "District of Columbia".to_string());
        map.insert("FL".to_string(), "Florida".to_string());
        map.insert("GA".to_string(), "Georgia".to_string());
        map.insert("GU".to_string(), "Guam".to_string());
        map.insert("HI".to_string(), "Hawaii".to_string());
        map.insert("ID".to_string(), "Idaho".to_string());
        map.insert("IL".to_string(), "Illinois".to_string());
        map.insert("IN".to_string(), "Indiana".to_string());
        map.insert("IA".to_string(), "Iowa".to_string());
        map.insert("KS".to_string(), "Kansas".to_string());
        map.insert("KY".to_string(), "Kentucky".to_string());
        map.insert("LA".to_string(), "Louisiana".to_string());
        map.insert("ME".to_string(), "Maine".to_string());
        map.insert("MD".to_string(), "Maryland".to_string());
        map.insert("MA".to_string(), "Massachusetts".to_string());
        map.insert("MI".to_string(), "Michigan".to_string());
        map.insert("MN".to_string(), "Minnesota".to_string());
        map.insert("MS".to_string(), "Mississippi".to_string());
        map.insert("MO".to_string(), "Missouri".to_string());
        map.insert("MT".to_string(), "Montana".to_string());
        map.insert("NE".to_string(), "Nebraska".to_string());
        map.insert("NE".to_string(), "Nevada".to_string());
        map.insert("NH".to_string(), "New Hampshire".to_string());
        map.insert("NJ".to_string(), "New Jersey".to_string());
        map.insert("NM".to_string(), "New Mexico".to_string());
        map.insert("NY".to_string(), "New York".to_string());
        map.insert("NC".to_string(), "North Carolina".to_string());
        map.insert("ND".to_string(), "North Dakota".to_string());
        map.insert("OH".to_string(), "Ohio".to_string());
        map.insert("OK".to_string(), "Oklahoma".to_string());
        map.insert("OR".to_string(), "Oregon".to_string());
        map.insert("PA".to_string(), "Pennsylvania".to_string());
        map.insert("PR".to_string(), "Puerto Rico".to_string());
        map.insert("RI".to_string(), "Rhode Island".to_string());
        map.insert("SC".to_string(), "South Carolina".to_string());
        map.insert("SD".to_string(), "South Dakota".to_string());
        map.insert("TN".to_string(), "Tennessee".to_string());
        map.insert("TX".to_string(), "Texas".to_string());
        map.insert("UT".to_string(), "Utah".to_string());
        map.insert("VT".to_string(), "Vermont".to_string());
        map.insert("VA".to_string(), "Virgina".to_string());
        map.insert("VI".to_string(), "Virgin Islands".to_string());
        map.insert("WA".to_string(), "Washington".to_string());
        map.insert("WV".to_string(), "West Virgina".to_string());
        map.insert("WI".to_string(), "Wisconsin".to_string());
        map.insert("WY".to_string(), "Wyoming".to_string());

        self.states = map;
    }

    #[tracing::instrument]
    fn new() -> Self {
        let mut sm = StatesMap {
            states: Default::default(),
        };
        // Populate the states.
        sm.populate();

        sm
    }

    #[tracing::instrument]
    pub fn shorthand(long: &str) -> String {
        let sm = StatesMap::new();
        for (key, value) in sm.states {
            if value == long {
                return key;
            }
        }

        long.to_string()
    }

    /// This function will try to match the full name for a state from an abreeviation,
    /// if one was given. Otherwise, it will return the existing string.
    /// This function is helpful when populating addresses.
    #[tracing::instrument]
    pub fn match_abreev_or_return_existing(s: &str) -> String {
        let sm = StatesMap::new();

        match sm.states.get(s.trim()) {
            Some(v) => v.to_string(),
            None => return s.trim().to_string(),
        }
    }
}
