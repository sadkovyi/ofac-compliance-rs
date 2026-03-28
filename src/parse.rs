use roxmltree::{Document, Node};

use crate::{
    Address, Alias, Country, DateOfBirth, Entry, Error, Identifier, List, Result, SourceField,
    normalize,
};

const NAMESPACE: &str =
    "https://sanctionslistservice.ofac.treas.gov/api/PublicationPreview/exports/XML";
const LEGACY_NAMESPACE: &str = "http://tempuri.org/sdnList.xsd";

/// Resource bounds for a single uncompressed XML document.
#[derive(Clone, Debug)]
pub struct ParseLimits {
    /// Maximum input bytes. Default: 128 MiB.
    pub max_bytes: usize,
    /// Maximum bytes in a primary name or alias. Default: 4 KiB.
    pub max_name_bytes: usize,
}

impl Default for ParseLimits {
    fn default() -> Self {
        Self {
            max_bytes: 128 * 1024 * 1024,
            max_name_bytes: 4096,
        }
    }
}

/// Parsed source records and publication information.
pub struct Publication {
    /// Entries in source order.
    pub entries: Vec<Entry>,
    /// Publisher date, retained verbatim.
    pub published_date: Option<String>,
    /// Publisher record count, if present.
    pub declared_count: Option<usize>,
}

/// Parse a complete standard OFAC XML document.
pub fn parse_xml(_list: List, bytes: &[u8], limits: &ParseLimits) -> Result<Publication> {
    limit(bytes.len(), limits.max_bytes, "document bytes")?;
    let xml = std::str::from_utf8(bytes).map_err(|_| Error::InvalidDocument("expected UTF-8"))?;
    // Reject even empty DTD declarations. External entities are never resolved.
    if xml.contains("<!DOCTYPE") {
        return Err(Error::InvalidDocument("DTD declarations are not supported"));
    }
    let document = Document::parse(xml)?;
    let root = document.root_element();
    if root.tag_name().name() != "sdnList"
        || !matches!(
            root.tag_name().namespace(),
            None | Some(NAMESPACE) | Some(LEGACY_NAMESPACE)
        )
    {
        return Err(Error::InvalidDocument(
            "expected a standard OFAC sdnList root",
        ));
    }
    let publication = child(root, "publshInformation")?;
    let published_date = publication
        .map(|n| text(n, "Publish_Date"))
        .transpose()?
        .flatten();
    let declared_count = publication
        .map(|n| text(n, "Record_Count"))
        .transpose()?
        .flatten()
        .map(|s| {
            s.parse::<usize>()
                .map_err(|_| Error::InvalidField("Record_Count"))
        })
        .transpose()?;
    let mut entries = Vec::new();
    for node in children(root, "sdnEntry") {
        let entry = parse_entry(node, limits)?;
        entries.push(entry);
    }
    if entries.is_empty() {
        return Err(Error::InvalidDocument("publication has no entries"));
    }
    Ok(Publication {
        entries,
        published_date,
        declared_count,
    })
}

fn parse_entry(node: Node<'_, '_>, limits: &ParseLimits) -> Result<Entry> {
    let uid = required(node, "uid")?;
    let entity_type = required(node, "sdnType")?;
    let first_name = text(node, "firstName")?;
    let last_name = required(node, "lastName")?;
    let name = combined_name(first_name.as_deref(), &last_name, limits)?;
    let aliases = records(node, "akaList", "aka")?
        .into_iter()
        .map(|alias| {
            let first_name = text(alias, "firstName")?;
            let last_name = required(alias, "lastName")?;
            Ok(Alias {
                uid: text(alias, "uid")?,
                name: combined_name(first_name.as_deref(), &last_name, limits)?,
                first_name,
                last_name,
                kind: text(alias, "type")?,
                category: text(alias, "category")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let identifiers = records(node, "idList", "id")?
        .into_iter()
        .map(|id| {
            Ok(Identifier {
                uid: text(id, "uid")?,
                kind: required(id, "idType")?,
                number: required(id, "idNumber")?,
                country: text(id, "idCountry")?,
                issue_date: text(id, "issueDate")?,
                expiration_date: text(id, "expirationDate")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let addresses = records(node, "addressList", "address")?
        .into_iter()
        .map(|a| {
            let lines = ["address1", "address2", "address3"]
                .into_iter()
                .map(|key| text(a, key))
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect();
            Ok(Address {
                uid: text(a, "uid")?,
                lines,
                city: text(a, "city")?,
                state_or_province: text(a, "stateOrProvince")?,
                postal_code: text(a, "postalCode")?,
                country: text(a, "country")?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let dates_of_birth = records(node, "dateOfBirthList", "dateOfBirthItem")?
        .into_iter()
        .map(|d| {
            Ok(DateOfBirth {
                uid: text(d, "uid")?,
                value: required(d, "dateOfBirth")?,
                main_entry: main_entry(d)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let programs = records(node, "programList", "program")?
        .into_iter()
        .map(|p| scalar(p, "program"))
        .collect::<Result<Vec<_>>>()?;
    const MODELED: &[&str] = &[
        "uid",
        "sdnType",
        "firstName",
        "lastName",
        "akaList",
        "idList",
        "addressList",
        "dateOfBirthList",
        "nationalityList",
        "citizenshipList",
        "programList",
        "remarks",
    ];
    let additional_fields = node
        .children()
        .filter(|n| n.is_element())
        .filter(|n| {
            n.tag_name().namespace() != node.tag_name().namespace()
                || !MODELED.contains(&n.tag_name().name())
        })
        .map(source_field)
        .collect();
    Ok(Entry {
        uid,
        entity_type,
        name,
        first_name,
        last_name,
        aliases,
        programs,
        identifiers,
        addresses,
        dates_of_birth,
        nationalities: countries(node, "nationalityList", "nationality")?,
        citizenships: countries(node, "citizenshipList", "citizenship")?,
        remarks: text(node, "remarks")?,
        additional_fields,
    })
}

fn countries(
    node: Node<'_, '_>,
    container: &'static str,
    item: &'static str,
) -> Result<Vec<Country>> {
    records(node, container, item)?
        .into_iter()
        .map(|n| {
            Ok(Country {
                uid: text(n, "uid")?,
                country: required(n, "country")?,
                main_entry: main_entry(n)?,
            })
        })
        .collect()
}

fn main_entry(node: Node<'_, '_>) -> Result<Option<bool>> {
    text(node, "mainEntry")?
        .map(|s| match s.as_str() {
            "true" | "1" => Ok(true),
            "false" | "0" => Ok(false),
            _ => Err(Error::InvalidField("mainEntry")),
        })
        .transpose()
}

fn combined_name(first: Option<&str>, last: &str, limits: &ParseLimits) -> Result<String> {
    let name = first.map_or_else(|| last.to_owned(), |first| format!("{first} {last}"));
    limit(name.len(), limits.max_name_bytes, "name bytes")?;
    if !normalize::searchable(&normalize::name(&name)) {
        return Err(Error::InvalidField("name"));
    }
    Ok(name)
}

fn source_field(node: Node<'_, '_>) -> SourceField {
    SourceField {
        name: node.tag_name().name().to_owned(),
        namespace: node.tag_name().namespace().map(str::to_owned),
        text: node
            .children()
            .filter(|n| n.is_text())
            .filter_map(|n| n.text())
            .collect(),
        children: node
            .children()
            .filter(|n| n.is_element())
            .map(source_field)
            .collect(),
    }
}

fn children<'a, 'input>(
    node: Node<'a, 'input>,
    name: &'static str,
) -> impl Iterator<Item = Node<'a, 'input>> {
    node.children().filter(move |n| {
        n.is_element()
            && n.tag_name().name() == name
            && n.tag_name().namespace() == node.tag_name().namespace()
    })
}

fn child<'a, 'input>(
    node: Node<'a, 'input>,
    name: &'static str,
) -> Result<Option<Node<'a, 'input>>> {
    let mut found = children(node, name);
    let value = found.next();
    if found.next().is_some() {
        return Err(Error::InvalidField(name));
    }
    Ok(value)
}

fn records<'a, 'input>(
    node: Node<'a, 'input>,
    container: &'static str,
    item: &'static str,
) -> Result<Vec<Node<'a, 'input>>> {
    match child(node, container)? {
        Some(node) => Ok(children(node, item).collect()),
        None => Ok(Vec::new()),
    }
}

fn scalar(node: Node<'_, '_>, name: &'static str) -> Result<String> {
    if node.children().any(|n| n.is_element()) {
        return Err(Error::InvalidField(name));
    }
    let text: String = node
        .children()
        .filter(|n| n.is_text())
        .filter_map(|n| n.text())
        .collect();
    Ok(text.trim().to_owned())
}

fn text(node: Node<'_, '_>, name: &'static str) -> Result<Option<String>> {
    child(node, name)?
        .map(|n| scalar(n, name))
        .transpose()
        .map(|v| v.filter(|s| !s.is_empty()))
}

fn required(node: Node<'_, '_>, name: &'static str) -> Result<String> {
    text(node, name)?.ok_or(Error::InvalidField(name))
}

fn limit(actual: usize, max: usize, resource: &'static str) -> Result<()> {
    if actual > max {
        return Err(Error::LimitExceeded {
            resource,
            limit: max,
        });
    }
    Ok(())
}
