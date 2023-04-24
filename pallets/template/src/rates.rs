extern crate serde;

use serde_derive::{Serialize, Deserialize};
use std::collections::HashMap;

#[derive(Deserialize, Serialize)]
pub struct Rates {
    rates: HashMap<String, f64>,
}

impl Rates {
    pub fn rate(&self, quote: &str) -> u32 {
        let r = self.rates.get(quote)
            .expect("It seems none of pairs contains such quote")
            .clone();

        let int = (r * 100.0) as i32;

        int as u32
    }
}

pub fn parse_json(json_string: &str) -> Rates {
    let rates: Rates = serde_json::from_str(json_string)
        .expect("Something went wrong while parsing request answer");
    
    rates
}
