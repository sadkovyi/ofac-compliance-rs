use std::fmt;

/// An OFAC publication. IDs are scoped to this value.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[non_exhaustive]
pub enum List {
    /// Specially Designated Nationals and Blocked Persons.
    Sdn,
    /// The consolidated non-SDN publication; program tags are retained per entry.
    Consolidated,
}

impl List {
    /// Official standard XML export endpoint.
    pub const fn url(self) -> &'static str {
        match self {
            Self::Sdn => {
                "https://sanctionslistservice.ofac.treas.gov/api/PublicationPreview/exports/SDN.XML"
            }
            Self::Consolidated => {
                "https://sanctionslistservice.ofac.treas.gov/api/PublicationPreview/exports/CONSOLIDATED.XML"
            }
        }
    }

    /// Stable machine-readable list key.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sdn => "sdn",
            Self::Consolidated => "consolidated",
        }
    }
}

impl fmt::Display for List {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A primary listing, with original source strings retained.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Entry {
    /// OFAC entity identifier, unique within the source list.
    pub uid: String,
    /// Source entity type, including types not known to this crate.
    pub entity_type: String,
    /// First name followed by last name, or the entity's complete name.
    pub name: String,
    /// Original first-name field, if present.
    pub first_name: Option<String>,
    /// Original last-name field (also used for organization names).
    pub last_name: String,
    /// Alternative names, including weak aliases.
    pub aliases: Vec<Alias>,
    /// Source program tags; do not interpret all tags as blocking sanctions.
    pub programs: Vec<String>,
    /// Passports, national IDs, digital currency addresses, and other identifiers.
    pub identifiers: Vec<Identifier>,
    /// Listed addresses.
    pub addresses: Vec<Address>,
    /// Dates, ranges, and partial dates retained verbatim.
    pub dates_of_birth: Vec<DateOfBirth>,
    /// Nationalities and their source main-entry flags.
    pub nationalities: Vec<Country>,
    /// Citizenships and their source main-entry flags.
    pub citizenships: Vec<Country>,
    /// Free-form source remarks.
    pub remarks: Option<String>,
    /// Unmodeled source fields, such as vessel data, titles, and places of birth.
    pub additional_fields: Vec<SourceField>,
}

/// An alternative name with its source quality and type.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Alias {
    /// Source alias ID, when present.
    pub uid: Option<String>,
    /// Combined name in first-name/last-name order.
    pub name: String,
    /// Original first-name field.
    pub first_name: Option<String>,
    /// Original last-name field.
    pub last_name: String,
    /// Source alias type, for example `a.k.a.`.
    pub kind: Option<String>,
    /// Source category, for example `strong` or `weak`.
    pub category: Option<String>,
}

impl Alias {
    /// Whether the source explicitly marks this alias as weak.
    pub fn is_weak(&self) -> bool {
        self.category
            .as_deref()
            .is_some_and(|s| s.eq_ignore_ascii_case("weak"))
    }
}

/// An identifier as published, without inferred validity or expiration rules.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Identifier {
    /// Source identifier record ID.
    pub uid: Option<String>,
    /// Identifier type, such as `Passport` or `Digital Currency Address - XBT`.
    pub kind: String,
    /// Original identifier value. Matching is case-sensitive after trimming.
    pub number: String,
    /// Issuing country, if provided.
    pub country: Option<String>,
    /// Issuance date, retained verbatim.
    pub issue_date: Option<String>,
    /// Expiration date, retained verbatim.
    pub expiration_date: Option<String>,
}

/// A postal address, retaining the source's free-form components.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Address {
    /// Source address record ID.
    pub uid: Option<String>,
    /// Address lines, in source order.
    pub lines: Vec<String>,
    /// City.
    pub city: Option<String>,
    /// State or province.
    pub state_or_province: Option<String>,
    /// Postal code.
    pub postal_code: Option<String>,
    /// Country.
    pub country: Option<String>,
}

/// A source date of birth, which may be a range or an incomplete date.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct DateOfBirth {
    /// Source record ID.
    pub uid: Option<String>,
    /// Original date text; no precision is invented.
    pub value: String,
    /// Whether OFAC marks this as the main entry.
    pub main_entry: Option<bool>,
}

/// A nationality or citizenship record.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Country {
    /// Source record ID.
    pub uid: Option<String>,
    /// Original country text.
    pub country: String,
    /// Source main-entry flag.
    pub main_entry: Option<bool>,
}

/// An unmodeled XML element. The complete original XML is also kept by the snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct SourceField {
    /// Element local name.
    pub name: String,
    /// Element namespace.
    pub namespace: Option<String>,
    /// Direct text nodes joined in document order.
    pub text: String,
    /// Child elements.
    pub children: Vec<SourceField>,
}
