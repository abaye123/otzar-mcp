//! Hebrew numerals as used in the catalog's `years` table (e.g. `תש"ל`, `תר"צ בערך`).

/// Numeric value of a Hebrew letter, final forms included.
pub fn letter_value(c: char) -> Option<i32> {
    Some(match c {
        'א' => 1,
        'ב' => 2,
        'ג' => 3,
        'ד' => 4,
        'ה' => 5,
        'ו' => 6,
        'ז' => 7,
        'ח' => 8,
        'ט' => 9,
        'י' => 10,
        'כ' | 'ך' => 20,
        'ל' => 30,
        'מ' | 'ם' => 40,
        'נ' | 'ן' => 50,
        'ס' => 60,
        'ע' => 70,
        'פ' | 'ף' => 80,
        'צ' | 'ץ' => 90,
        'ק' => 100,
        'ר' => 200,
        'ש' => 300,
        'ת' => 400,
        _ => return None,
    })
}

/// Qualifier words that appear around year numerals and are not numerals themselves.
const QUALIFIERS: &[&str] = &[
    "אחר", "אחרי", "לפני", "בערך", "לערך", "ערך", "בין", "או", "עד", "בסביבות", "קודם",
];

fn is_geresh(c: char) -> bool {
    matches!(c, '"' | '\'' | '״' | '׳' | '`')
}

/// Parses a year label into an approximate Anno Mundi year.
///
/// * `תש"ל` -> 5730 (thousands are implied when the value is below 1000)
/// * `ר"מ לפני` -> 3521 (240 before the common era)
/// * `1850` -> 5610 (Gregorian digits)
///
/// Returns `None` when the label holds no recognizable numeral.
pub fn parse_year(label: &str) -> Option<i32> {
    let label = label.trim_start_matches('\u{feff}');
    let before_era = label.contains("לפני");
    for token in label.split(|c: char| c.is_whitespace() || matches!(c, '-' | '[' | ']' | '(' | ')' | '?')) {
        if token.is_empty() || QUALIFIERS.contains(&token) {
            continue;
        }
        if token.chars().all(|c| c.is_ascii_digit()) {
            if let Ok(year) = token.parse::<i32>()
                && (500..2200).contains(&year)
            {
                return Some(year + 3760);
            }
            continue;
        }
        if !token.chars().all(|c| letter_value(c).is_some() || is_geresh(c)) {
            continue;
        }
        let sum: i32 = token.chars().filter_map(letter_value).sum();
        if sum == 0 {
            continue;
        }
        if before_era {
            return Some(3761 - sum);
        }
        return Some(if sum < 1000 { sum + 5000 } else { sum });
    }
    None
}

/// Converts a user-supplied year (Gregorian like 1850 or AM like 5610) to Anno Mundi.
pub fn normalize_year(year: i32) -> i32 {
    if year < 3000 { year + 3760 } else { year }
}

/// Approximate Gregorian year for an Anno Mundi year (ignores the Tishrei offset).
pub fn am_to_ce(am: i32) -> i32 {
    am - 3760
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_common_labels() {
        assert_eq!(parse_year("תש\"ל"), Some(5730));
        assert_eq!(parse_year("תשפ\"ז"), Some(5787));
        assert_eq!(parse_year("תר\"צ בערך"), Some(5690));
        assert_eq!(parse_year("אחר תש\"ז"), Some(5707));
        assert_eq!(parse_year("שנד לערך"), Some(5354));
        assert_eq!(parse_year("ת'"), Some(5400));
        assert_eq!(parse_year("ר\"מ לפני"), Some(3521));
        assert_eq!(parse_year("1850"), Some(5610));
        assert_eq!(parse_year(""), None);
        assert_eq!(parse_year("[?]"), None);
    }

    #[test]
    fn normalizes_user_years() {
        assert_eq!(normalize_year(1850), 5610);
        assert_eq!(normalize_year(5610), 5610);
        assert_eq!(am_to_ce(5730), 1970);
    }
}
