//! English country names for location search (`tiles::search_places`).
//! GeoNames stores ISO 3166-1 alpha-2; queries use names ("chile") or codes.

/// English short name for an ISO 3166-1 alpha-2 code.
pub fn name(code: &str) -> Option<&'static str> {
    let code = code.trim();
    if code.len() != 2 {
        return None;
    }
    let key = [
        code.as_bytes()[0].to_ascii_uppercase(),
        code.as_bytes()[1].to_ascii_uppercase(),
    ];
    NAMES
        .binary_search_by_key(&key.as_slice(), |row| row.0.as_bytes())
        .ok()
        .map(|i| NAMES[i].1)
}

/// Whether `token` (already lowercase) is an alias or the English short name
/// of a country. Those tokens filter by country (or a same-named region,
/// Georgia) and must not match city-name prefixes: `chile` is not Chilecito.
pub fn is_country_name_token(token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    if alias_code(token).is_some() {
        return true;
    }
    NAMES.iter().any(|(_, n)| n.eq_ignore_ascii_case(token))
}

/// Whether `token` (already lowercase) names this ISO country code.
pub fn token_matches(code: &str, token: &str) -> bool {
    if token.is_empty() {
        return false;
    }
    if let Some(alias) = alias_code(token) {
        return code.eq_ignore_ascii_case(alias);
    }
    if token.len() == 2 {
        return code.eq_ignore_ascii_case(token);
    }
    let Some(name) = name(code) else {
        return false;
    };
    let name = name.to_lowercase();
    name.split(|c: char| !c.is_alphanumeric())
        .any(|word| !word.is_empty() && word.starts_with(token))
        || (token.len() >= 4 && name.contains(token))
}

fn alias_code(token: &str) -> Option<&'static str> {
    match token {
        "usa" | "america" => Some("US"),
        "uk" | "britain" | "england" => Some("GB"),
        "uae" => Some("AE"),
        "russia" => Some("RU"),
        "southkorea" | "korea" => Some("KR"),
        "northkorea" => Some("KP"),
        "vietnam" => Some("VN"),
        "bolivia" => Some("BO"),
        "tanzania" => Some("TZ"),
        "laos" => Some("LA"),
        "moldova" => Some("MD"),
        "syria" => Some("SY"),
        "venezuela" => Some("VE"),
        "iran" => Some("IR"),
        "holland" => Some("NL"),
        "czechia" => Some("CZ"),
        _ => None,
    }
}

/// Sorted by ISO code. Names are the usual English short form a search types.
const NAMES: &[(&str, &str)] = &[
    ("AD", "Andorra"),
    ("AE", "United Arab Emirates"),
    ("AF", "Afghanistan"),
    ("AG", "Antigua and Barbuda"),
    ("AI", "Anguilla"),
    ("AL", "Albania"),
    ("AM", "Armenia"),
    ("AO", "Angola"),
    ("AR", "Argentina"),
    ("AS", "American Samoa"),
    ("AT", "Austria"),
    ("AU", "Australia"),
    ("AW", "Aruba"),
    ("AZ", "Azerbaijan"),
    ("BA", "Bosnia and Herzegovina"),
    ("BB", "Barbados"),
    ("BD", "Bangladesh"),
    ("BE", "Belgium"),
    ("BF", "Burkina Faso"),
    ("BG", "Bulgaria"),
    ("BH", "Bahrain"),
    ("BI", "Burundi"),
    ("BJ", "Benin"),
    ("BM", "Bermuda"),
    ("BN", "Brunei"),
    ("BO", "Bolivia"),
    ("BR", "Brazil"),
    ("BS", "Bahamas"),
    ("BT", "Bhutan"),
    ("BW", "Botswana"),
    ("BY", "Belarus"),
    ("BZ", "Belize"),
    ("CA", "Canada"),
    ("CD", "Congo"),
    ("CF", "Central African Republic"),
    ("CG", "Congo"),
    ("CH", "Switzerland"),
    ("CI", "Ivory Coast"),
    ("CL", "Chile"),
    ("CM", "Cameroon"),
    ("CN", "China"),
    ("CO", "Colombia"),
    ("CR", "Costa Rica"),
    ("CU", "Cuba"),
    ("CV", "Cabo Verde"),
    ("CY", "Cyprus"),
    ("CZ", "Czech Republic"),
    ("DE", "Germany"),
    ("DJ", "Djibouti"),
    ("DK", "Denmark"),
    ("DM", "Dominica"),
    ("DO", "Dominican Republic"),
    ("DZ", "Algeria"),
    ("EC", "Ecuador"),
    ("EE", "Estonia"),
    ("EG", "Egypt"),
    ("ER", "Eritrea"),
    ("ES", "Spain"),
    ("ET", "Ethiopia"),
    ("FI", "Finland"),
    ("FJ", "Fiji"),
    ("FK", "Falkland Islands"),
    ("FM", "Micronesia"),
    ("FO", "Faroe Islands"),
    ("FR", "France"),
    ("GA", "Gabon"),
    ("GB", "United Kingdom"),
    ("GD", "Grenada"),
    ("GE", "Georgia"),
    ("GF", "French Guiana"),
    ("GH", "Ghana"),
    ("GI", "Gibraltar"),
    ("GL", "Greenland"),
    ("GM", "Gambia"),
    ("GN", "Guinea"),
    ("GP", "Guadeloupe"),
    ("GQ", "Equatorial Guinea"),
    ("GR", "Greece"),
    ("GT", "Guatemala"),
    ("GU", "Guam"),
    ("GW", "Guinea-Bissau"),
    ("GY", "Guyana"),
    ("HK", "Hong Kong"),
    ("HN", "Honduras"),
    ("HR", "Croatia"),
    ("HT", "Haiti"),
    ("HU", "Hungary"),
    ("ID", "Indonesia"),
    ("IE", "Ireland"),
    ("IL", "Israel"),
    ("IN", "India"),
    ("IQ", "Iraq"),
    ("IR", "Iran"),
    ("IS", "Iceland"),
    ("IT", "Italy"),
    ("JM", "Jamaica"),
    ("JO", "Jordan"),
    ("JP", "Japan"),
    ("KE", "Kenya"),
    ("KG", "Kyrgyzstan"),
    ("KH", "Cambodia"),
    ("KI", "Kiribati"),
    ("KM", "Comoros"),
    ("KN", "Saint Kitts and Nevis"),
    ("KP", "North Korea"),
    ("KR", "South Korea"),
    ("KW", "Kuwait"),
    ("KY", "Cayman Islands"),
    ("KZ", "Kazakhstan"),
    ("LA", "Laos"),
    ("LB", "Lebanon"),
    ("LC", "Saint Lucia"),
    ("LI", "Liechtenstein"),
    ("LK", "Sri Lanka"),
    ("LR", "Liberia"),
    ("LS", "Lesotho"),
    ("LT", "Lithuania"),
    ("LU", "Luxembourg"),
    ("LV", "Latvia"),
    ("LY", "Libya"),
    ("MA", "Morocco"),
    ("MC", "Monaco"),
    ("MD", "Moldova"),
    ("ME", "Montenegro"),
    ("MF", "Saint Martin"),
    ("MG", "Madagascar"),
    ("MH", "Marshall Islands"),
    ("MK", "North Macedonia"),
    ("ML", "Mali"),
    ("MM", "Myanmar"),
    ("MN", "Mongolia"),
    ("MO", "Macao"),
    ("MP", "Northern Mariana Islands"),
    ("MQ", "Martinique"),
    ("MR", "Mauritania"),
    ("MS", "Montserrat"),
    ("MT", "Malta"),
    ("MU", "Mauritius"),
    ("MV", "Maldives"),
    ("MW", "Malawi"),
    ("MX", "Mexico"),
    ("MY", "Malaysia"),
    ("MZ", "Mozambique"),
    ("NA", "Namibia"),
    ("NC", "New Caledonia"),
    ("NE", "Niger"),
    ("NF", "Norfolk Island"),
    ("NG", "Nigeria"),
    ("NI", "Nicaragua"),
    ("NL", "Netherlands"),
    ("NO", "Norway"),
    ("NP", "Nepal"),
    ("NR", "Nauru"),
    ("NU", "Niue"),
    ("NZ", "New Zealand"),
    ("OM", "Oman"),
    ("PA", "Panama"),
    ("PE", "Peru"),
    ("PF", "French Polynesia"),
    ("PG", "Papua New Guinea"),
    ("PH", "Philippines"),
    ("PK", "Pakistan"),
    ("PL", "Poland"),
    ("PM", "Saint Pierre and Miquelon"),
    ("PR", "Puerto Rico"),
    ("PS", "Palestine"),
    ("PT", "Portugal"),
    ("PW", "Palau"),
    ("PY", "Paraguay"),
    ("QA", "Qatar"),
    ("RE", "Reunion"),
    ("RO", "Romania"),
    ("RS", "Serbia"),
    ("RU", "Russia"),
    ("RW", "Rwanda"),
    ("SA", "Saudi Arabia"),
    ("SB", "Solomon Islands"),
    ("SC", "Seychelles"),
    ("SD", "Sudan"),
    ("SE", "Sweden"),
    ("SG", "Singapore"),
    ("SI", "Slovenia"),
    ("SK", "Slovakia"),
    ("SL", "Sierra Leone"),
    ("SM", "San Marino"),
    ("SN", "Senegal"),
    ("SO", "Somalia"),
    ("SR", "Suriname"),
    ("SS", "South Sudan"),
    ("ST", "Sao Tome and Principe"),
    ("SV", "El Salvador"),
    ("SX", "Sint Maarten"),
    ("SY", "Syria"),
    ("SZ", "Eswatini"),
    ("TC", "Turks and Caicos Islands"),
    ("TD", "Chad"),
    ("TG", "Togo"),
    ("TH", "Thailand"),
    ("TJ", "Tajikistan"),
    ("TK", "Tokelau"),
    ("TL", "Timor-Leste"),
    ("TM", "Turkmenistan"),
    ("TN", "Tunisia"),
    ("TO", "Tonga"),
    ("TR", "Turkey"),
    ("TT", "Trinidad and Tobago"),
    ("TV", "Tuvalu"),
    ("TW", "Taiwan"),
    ("TZ", "Tanzania"),
    ("UA", "Ukraine"),
    ("UG", "Uganda"),
    ("US", "United States"),
    ("UY", "Uruguay"),
    ("UZ", "Uzbekistan"),
    ("VA", "Vatican"),
    ("VC", "Saint Vincent and the Grenadines"),
    ("VE", "Venezuela"),
    ("VG", "British Virgin Islands"),
    ("VI", "US Virgin Islands"),
    ("VN", "Vietnam"),
    ("VU", "Vanuatu"),
    ("WF", "Wallis and Futuna"),
    ("WS", "Samoa"),
    ("XK", "Kosovo"),
    ("YE", "Yemen"),
    ("YT", "Mayotte"),
    ("ZA", "South Africa"),
    ("ZM", "Zambia"),
    ("ZW", "Zimbabwe"),
];

#[cfg(test)]
mod tests {
    use super::{is_country_name_token, name, token_matches};

    #[test]
    fn chile_is_named_and_matched() {
        assert_eq!(name("CL"), Some("Chile"));
        assert_eq!(name("cl"), Some("Chile"));
        assert!(token_matches("CL", "chile"));
        assert!(token_matches("CL", "cl"));
        assert!(!token_matches("US", "chile"));
        assert!(token_matches("US", "usa"));
        assert!(token_matches("US", "united"));
        assert!(token_matches("GB", "uk"));
        assert!(is_country_name_token("chile"));
        assert!(is_country_name_token("usa"));
        assert!(is_country_name_token("uk"));
        assert!(!is_country_name_token("united"));
        assert!(!is_country_name_token("cl"));
        assert!(!is_country_name_token("santiago"));
    }

    #[test]
    fn names_stay_sorted_for_binary_search() {
        assert!(super::NAMES.windows(2).all(|w| w[0].0 < w[1].0));
    }
}
