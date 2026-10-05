use std::collections::HashSet;

use roxmltree::{Document, Node, ParsingOptions};

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
    /// Maximum XML nodes, including text. Default: 4,000,000.
    pub max_nodes: u32,
    /// Maximum nesting depth. Default: 64.
    pub max_depth: usize,
    /// Maximum entity records. Default: 250,000.
    pub max_entries: usize,
    /// Maximum text bytes in any one XML text node. Default: 1 MiB.
    pub max_text_bytes: usize,
    /// Maximum bytes in a primary name or alias. Default: 4 KiB.
    pub max_name_bytes: usize,
}

impl Default for ParseLimits {
    fn default() -> Self {
        Self {
            max_bytes: 128 * 1024 * 1024,
            max_nodes: 4_000_000,
            max_depth: 64,
            max_entries: 250_000,
            max_text_bytes: 1024 * 1024,
            max_name_bytes: 4096,
        }
    }
}

pub(crate) struct Parsed {
    pub entries: Vec<Entry>,
    pub published_date: Option<String>,
    pub declared_count: Option<usize>,
}

pub(crate) fn parse(list: List, bytes: &[u8], limits: &ParseLimits) -> Result<Parsed> {
    limit(bytes.len(), limits.max_bytes, "document bytes")?;
    let xml = std::str::from_utf8(bytes).map_err(|_| Error::InvalidDocument("expected UTF-8"))?;
    // Reject even empty DTD declarations. External entities are never resolved.
    if xml.contains("<!DOCTYPE") {
        return Err(Error::InvalidDocument("DTD declarations are not supported"));
    }
    let document = Document::parse_with_options(
        xml,
        ParsingOptions {
            allow_dtd: false,
            nodes_limit: limits.max_nodes,
        },
    )?;
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
    let mut pending = vec![(root, 1)];
    while let Some((node, depth)) = pending.pop() {
        limit(depth, limits.max_depth, "XML depth")?;
        if node.is_element()
            && node.tag_name().name() == "sdnEntry"
            && (node.parent() != Some(root)
                || node.tag_name().namespace() != root.tag_name().namespace())
        {
            return Err(Error::InvalidDocument(
                "unexpected entity location or namespace",
            ));
        }
        if node.is_text() {
            limit(
                node.text().unwrap_or_default().len(),
                limits.max_text_bytes,
                "text bytes",
            )?;
        }
        pending.extend(node.children().map(|child| (child, depth + 1)));
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
    let mut identifiers = HashSet::new();
    for node in children(root, "sdnEntry") {
        limit(entries.len() + 1, limits.max_entries, "entries")?;
        let entry = parse_entry(node, limits)?;
        if !identifiers.insert(entry.uid.clone()) {
            return Err(Error::DuplicateEntity(list));
        }
        entries.push(entry);
    }
    if entries.is_empty() {
        return Err(Error::InvalidDocument("publication has no entries"));
    }
    if let Some(declared) = declared_count
        && declared != entries.len()
    {
        return Err(Error::RecordCount {
            declared,
            actual: entries.len(),
        });
    }
    Ok(Parsed {
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
        .map(|p| {
            let value = scalar(p, "program")?;
            if value.is_empty() {
                return Err(Error::InvalidField("program"));
            }
            Ok(value)
        })
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
    reject_foreign_field(node, name)?;
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
        Some(node) => {
            reject_foreign_field(node, item)?;
            Ok(children(node, item).collect())
        }
        None => Ok(Vec::new()),
    }
}

fn reject_foreign_field(node: Node<'_, '_>, name: &'static str) -> Result<()> {
    if node.children().any(|n| {
        n.is_element()
            && n.tag_name().name() == name
            && n.tag_name().namespace() != node.tag_name().namespace()
    }) {
        return Err(Error::InvalidField(name));
    }
    Ok(())
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
