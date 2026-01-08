// Tax configuration module for MoneyMQ
// Provides tax rate lookups for sales tax, VAT, GST, and digital services taxes.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

/// Tax rate lookup result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxRate {
    pub country_code: String,
    pub country_name: String,
    pub state_code: Option<String>,
    pub state_name: Option<String>,
    pub tax_type: String,
    pub rate: f64,
    pub currency: String,
    pub digital_goods_taxable: bool,
    pub dst_rate: Option<f64>,
    pub notes: Option<String>,
}

/// Country tax configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CountryConfig {
    pub name: String,
    pub currency: String,
    pub tax_type: String,
    #[serde(default)]
    pub federal_rate: Option<f64>,
    #[serde(default)]
    pub has_state_taxes: Option<bool>,
    #[serde(default)]
    pub digital_goods_taxable: Option<serde_yml::Value>,
    #[serde(default)]
    pub dst_rate: Option<f64>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub rates: Option<RatesConfig>,
    #[serde(default)]
    pub states: Option<IndexMap<String, StateConfig>>,
    #[serde(default)]
    pub provinces: Option<IndexMap<String, ProvinceConfig>>,
    #[serde(default)]
    pub eu_member: Option<bool>,
}

/// VAT/GST rate tiers
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RatesConfig {
    #[serde(default)]
    pub standard: Option<f64>,
    #[serde(default)]
    pub reduced: Option<serde_yml::Value>,
    #[serde(default)]
    pub super_reduced: Option<f64>,
    #[serde(default)]
    pub parking: Option<f64>,
    #[serde(default)]
    pub zero: Option<f64>,
}

/// US state tax configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateConfig {
    pub name: String,
    #[serde(default)]
    pub rate: Option<f64>,
    #[serde(default)]
    pub has_local_tax: Option<bool>,
    #[serde(default)]
    pub avg_local_rate: Option<f64>,
    #[serde(default)]
    pub combined_avg: Option<f64>,
    #[serde(default)]
    pub digital_goods_taxable: Option<bool>,
    #[serde(default)]
    pub notes: Option<String>,
    #[serde(default)]
    pub tax_type: Option<String>,
    // For Brazil states
    #[serde(default)]
    pub icms: Option<f64>,
    // For Australia
    #[serde(default)]
    pub additional_tax: Option<f64>,
}

/// Canadian province tax configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProvinceConfig {
    pub name: String,
    #[serde(default)]
    pub gst: Option<f64>,
    #[serde(default)]
    pub pst: Option<f64>,
    #[serde(default)]
    pub hst: Option<f64>,
    #[serde(default)]
    pub qst: Option<f64>,
    pub total: f64,
    pub tax_type: String,
}

/// Full tax data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxData {
    pub version: String,
    pub last_updated: String,
    pub countries: IndexMap<String, CountryConfig>,
    #[serde(default)]
    pub digital_services_tax: Option<serde_yml::Value>,
    #[serde(default)]
    pub tax_categories: Option<serde_yml::Value>,
    #[serde(default)]
    pub registration_thresholds: Option<serde_yml::Value>,
    #[serde(default)]
    pub special_zones: Option<serde_yml::Value>,
}

impl TaxData {
    /// Load tax data from embedded YAML
    pub fn load() -> Result<Self, String> {
        let yaml_content = include_str!("data.yaml");
        serde_yml::from_str(yaml_content).map_err(|e| format!("Failed to parse tax data: {}", e))
    }

    /// Get tax rate for a country (federal/standard rate)
    pub fn get_country_rate(&self, country_code: &str) -> Option<TaxRate> {
        let country = self.countries.get(country_code)?;

        let rate = if let Some(rates) = &country.rates {
            rates.standard.unwrap_or(0.0)
        } else {
            country.federal_rate.unwrap_or(0.0)
        };

        let digital_taxable = match &country.digital_goods_taxable {
            Some(serde_yml::Value::Bool(b)) => *b,
            Some(serde_yml::Value::String(s)) => s != "false",
            _ => true,
        };

        Some(TaxRate {
            country_code: country_code.to_string(),
            country_name: country.name.clone(),
            state_code: None,
            state_name: None,
            tax_type: country.tax_type.clone(),
            rate,
            currency: country.currency.clone(),
            digital_goods_taxable: digital_taxable,
            dst_rate: country.dst_rate,
            notes: country.notes.clone(),
        })
    }

    /// Get tax rate for a US state
    pub fn get_us_state_rate(&self, state_code: &str) -> Option<TaxRate> {
        let us = self.countries.get("US")?;
        let states = us.states.as_ref()?;
        let state = states.get(state_code)?;

        Some(TaxRate {
            country_code: "US".to_string(),
            country_name: "United States".to_string(),
            state_code: Some(state_code.to_string()),
            state_name: Some(state.name.clone()),
            tax_type: state
                .tax_type
                .clone()
                .unwrap_or_else(|| "sales_tax".to_string()),
            rate: state.rate.unwrap_or(0.0),
            currency: "USD".to_string(),
            digital_goods_taxable: state.digital_goods_taxable.unwrap_or(false),
            dst_rate: None,
            notes: state.notes.clone(),
        })
    }

    /// Get combined tax rate for a US state (state + avg local)
    pub fn get_us_state_combined_rate(&self, state_code: &str) -> Option<f64> {
        let us = self.countries.get("US")?;
        let states = us.states.as_ref()?;
        let state = states.get(state_code)?;
        state.combined_avg.or(state.rate)
    }

    /// Get tax rate for a Canadian province
    pub fn get_ca_province_rate(&self, province_code: &str) -> Option<TaxRate> {
        let ca = self.countries.get("CA")?;
        let provinces = ca.provinces.as_ref()?;
        let province = provinces.get(province_code)?;

        Some(TaxRate {
            country_code: "CA".to_string(),
            country_name: "Canada".to_string(),
            state_code: Some(province_code.to_string()),
            state_name: Some(province.name.clone()),
            tax_type: province.tax_type.clone(),
            rate: province.total,
            currency: "CAD".to_string(),
            digital_goods_taxable: true,
            dst_rate: None,
            notes: None,
        })
    }

    /// Get tax rate for any jurisdiction (country, optionally with state/province)
    pub fn get_rate(&self, country_code: &str, subdivision_code: Option<&str>) -> Option<TaxRate> {
        match (country_code, subdivision_code) {
            ("US", Some(state)) => self.get_us_state_rate(state),
            ("CA", Some(province)) => self.get_ca_province_rate(province),
            (country, _) => self.get_country_rate(country),
        }
    }

    /// Check if a country is an EU member
    pub fn is_eu_member(&self, country_code: &str) -> bool {
        self.countries
            .get(country_code)
            .and_then(|c| c.eu_member)
            .unwrap_or(false)
    }

    /// Get all EU countries
    pub fn get_eu_countries(&self) -> Vec<&str> {
        self.countries
            .iter()
            .filter(|(_, config)| config.eu_member.unwrap_or(false))
            .map(|(code, _)| code.as_str())
            .collect()
    }

    /// Get all US states with sales tax
    pub fn get_us_states_with_tax(&self) -> Vec<(&str, f64)> {
        let Some(us) = self.countries.get("US") else {
            return vec![];
        };
        let Some(states) = &us.states else {
            return vec![];
        };

        states
            .iter()
            .filter_map(|(code, state)| {
                let rate = state.rate?;
                if rate > 0.0 {
                    Some((code.as_str(), rate))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get all US states where digital goods are taxable
    pub fn get_us_digital_tax_states(&self) -> Vec<&str> {
        let Some(us) = self.countries.get("US") else {
            return vec![];
        };
        let Some(states) = &us.states else {
            return vec![];
        };

        states
            .iter()
            .filter(|(_, state)| state.digital_goods_taxable.unwrap_or(false))
            .map(|(code, _)| code.as_str())
            .collect()
    }

    /// Check if digital services tax applies
    pub fn has_digital_services_tax(&self, country_code: &str) -> bool {
        self.countries
            .get(country_code)
            .and_then(|c| c.dst_rate)
            .is_some()
    }

    /// Get list of all country codes
    pub fn get_country_codes(&self) -> Vec<&str> {
        self.countries.keys().map(|s| s.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_tax_data() {
        let data = TaxData::load().expect("Failed to load tax data");
        assert!(!data.countries.is_empty());
        assert!(data.countries.contains_key("US"));
        assert!(data.countries.contains_key("GB"));
        assert!(data.countries.contains_key("DE"));
    }

    #[test]
    fn test_us_federal_rate() {
        let data = TaxData::load().unwrap();
        let rate = data.get_country_rate("US").unwrap();
        assert_eq!(rate.rate, 0.0); // No federal sales tax
        assert_eq!(rate.tax_type, "sales_tax");
    }

    #[test]
    fn test_us_state_rates() {
        let data = TaxData::load().unwrap();

        // California - highest state rate
        let ca = data.get_us_state_rate("CA").unwrap();
        assert_eq!(ca.rate, 7.25);
        assert!(!ca.digital_goods_taxable);

        // Oregon - no sales tax
        let or = data.get_us_state_rate("OR").unwrap();
        assert_eq!(or.rate, 0.0);

        // Texas
        let tx = data.get_us_state_rate("TX").unwrap();
        assert_eq!(tx.rate, 6.25);
        assert!(tx.digital_goods_taxable);
    }

    #[test]
    fn test_us_combined_rates() {
        let data = TaxData::load().unwrap();

        // Tennessee has high combined rate
        let tn = data.get_us_state_combined_rate("TN").unwrap();
        assert_eq!(tn, 9.55);

        // California combined
        let ca = data.get_us_state_combined_rate("CA").unwrap();
        assert_eq!(ca, 8.82);
    }

    #[test]
    fn test_canada_provinces() {
        let data = TaxData::load().unwrap();

        // Ontario HST
        let on = data.get_ca_province_rate("ON").unwrap();
        assert_eq!(on.rate, 13.0);
        assert_eq!(on.tax_type, "HST");

        // Alberta GST only
        let ab = data.get_ca_province_rate("AB").unwrap();
        assert_eq!(ab.rate, 5.0);
        assert_eq!(ab.tax_type, "GST");

        // Quebec GST+QST
        let qc = data.get_ca_province_rate("QC").unwrap();
        assert_eq!(qc.rate, 14.975);
    }

    #[test]
    fn test_eu_vat_rates() {
        let data = TaxData::load().unwrap();

        // Germany
        let de = data.get_country_rate("DE").unwrap();
        assert_eq!(de.rate, 19.0);
        assert_eq!(de.tax_type, "VAT");

        // Hungary - highest EU VAT
        let hu = data.get_country_rate("HU").unwrap();
        assert_eq!(hu.rate, 27.0);

        // Luxembourg - lowest EU VAT
        let lu = data.get_country_rate("LU").unwrap();
        assert_eq!(lu.rate, 17.0);
    }

    #[test]
    fn test_uk_vat() {
        let data = TaxData::load().unwrap();
        let gb = data.get_country_rate("GB").unwrap();
        assert_eq!(gb.rate, 20.0);
        assert_eq!(gb.dst_rate, Some(2.0));
        assert!(!data.is_eu_member("GB")); // Post-Brexit
    }

    #[test]
    fn test_eu_membership() {
        let data = TaxData::load().unwrap();

        assert!(data.is_eu_member("DE"));
        assert!(data.is_eu_member("FR"));
        assert!(data.is_eu_member("IT"));
        assert!(!data.is_eu_member("GB"));
        assert!(!data.is_eu_member("US"));
        assert!(!data.is_eu_member("CH"));

        let eu_countries = data.get_eu_countries();
        assert!(eu_countries.contains(&"DE"));
        assert!(eu_countries.contains(&"FR"));
        assert!(!eu_countries.contains(&"GB"));
    }

    #[test]
    fn test_digital_services_tax() {
        let data = TaxData::load().unwrap();

        // Countries with DST
        assert!(data.has_digital_services_tax("FR"));
        assert!(data.has_digital_services_tax("GB"));
        assert!(data.has_digital_services_tax("IT"));
        assert!(data.has_digital_services_tax("ES"));

        // Countries without DST
        assert!(!data.has_digital_services_tax("US"));
        assert!(!data.has_digital_services_tax("DE"));
    }

    #[test]
    fn test_australia_gst() {
        let data = TaxData::load().unwrap();
        let au = data.get_country_rate("AU").unwrap();
        assert_eq!(au.rate, 10.0);
        assert_eq!(au.tax_type, "GST");
    }

    #[test]
    fn test_japan_consumption_tax() {
        let data = TaxData::load().unwrap();
        let jp = data.get_country_rate("JP").unwrap();
        assert_eq!(jp.rate, 10.0);
        assert_eq!(jp.tax_type, "consumption_tax");
    }

    #[test]
    fn test_get_rate_combined() {
        let data = TaxData::load().unwrap();

        // US with state
        let tx = data.get_rate("US", Some("TX")).unwrap();
        assert_eq!(tx.state_code, Some("TX".to_string()));
        assert_eq!(tx.rate, 6.25);

        // Canada with province
        let on = data.get_rate("CA", Some("ON")).unwrap();
        assert_eq!(on.state_code, Some("ON".to_string()));
        assert_eq!(on.rate, 13.0);

        // Other country (subdivision ignored)
        let de = data.get_rate("DE", Some("BY")).unwrap();
        assert!(de.state_code.is_none());
        assert_eq!(de.rate, 19.0);
    }

    #[test]
    fn test_us_states_with_tax() {
        let data = TaxData::load().unwrap();
        let states = data.get_us_states_with_tax();

        // Should include CA, TX, NY, etc.
        assert!(states.iter().any(|(code, _)| *code == "CA"));
        assert!(states.iter().any(|(code, _)| *code == "TX"));
        assert!(states.iter().any(|(code, _)| *code == "NY"));

        // Should NOT include OR, DE, MT, NH, AK (0% state rate)
        assert!(!states.iter().any(|(code, _)| *code == "OR"));
        assert!(!states.iter().any(|(code, _)| *code == "DE"));
    }

    #[test]
    fn test_us_digital_tax_states() {
        let data = TaxData::load().unwrap();
        let digital_states = data.get_us_digital_tax_states();

        // These states tax digital goods
        assert!(digital_states.contains(&"TX"));
        assert!(digital_states.contains(&"NY"));
        assert!(digital_states.contains(&"WA"));

        // These don't
        assert!(!digital_states.contains(&"CA"));
        assert!(!digital_states.contains(&"FL"));
    }

    #[test]
    fn test_no_tax_jurisdictions() {
        let data = TaxData::load().unwrap();

        // Hong Kong has no sales tax
        let hk = data.get_country_rate("HK").unwrap();
        assert_eq!(hk.rate, 0.0);
        assert!(!hk.digital_goods_taxable);
    }

    #[test]
    fn test_all_countries_valid() {
        let data = TaxData::load().unwrap();
        let codes = data.get_country_codes();

        // Should have substantial coverage
        assert!(codes.len() >= 50);

        // All should return valid rates
        for code in codes {
            let rate = data.get_country_rate(code);
            assert!(rate.is_some(), "Missing rate for country: {}", code);
        }
    }
}
