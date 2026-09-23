use std::fs;
use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;

const ADR_DIR: &str = "docs/adr";
const INDEX_PATH: &str = "docs/adr/README.md";
const ABSENT_ADR_NUMBER: u16 = 9; // docs/adr/README.md:106 documents the intentional gap.

static ADR_NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"ADR-(\d{3})").expect("ADR number pattern is valid"));
static ADR_FILE_NUMBER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^adr-(\d{3})-.+\.md$").expect("ADR filename pattern is valid"));
static MARKDOWN_LINK: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[[^\]]+\]\(([^)]+)\)").expect("link pattern is valid"));
static STATUS_ADR_LINK: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\[ADR-(\d{3})\]\(([^)]+)\)").expect("Status ADR link pattern is valid")
});

#[derive(Clone, Debug)]
struct AdrFile {
    filename: String,
    contents: String,
}

#[derive(Clone, Debug)]
struct IndexRow {
    label: String,
    target: String,
    line: usize,
}

fn adr_number(filename: &str) -> Option<u16> {
    ADR_FILE_NUMBER
        .captures(filename)
        .and_then(|captures| captures.get(1))
        .and_then(|number| number.as_str().parse().ok())
}

fn resolve_adr_target(target: &str) -> Option<String> {
    let mut filename = None;
    for component in Path::new(target).components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::Normal(name) => {
                if filename.is_some() {
                    return None;
                }
                filename = name.to_str().map(str::to_owned);
            }
            _ => return None,
        }
    }
    filename
}

fn parse_index(contents: &str) -> Vec<IndexRow> {
    contents
        .lines()
        .enumerate()
        .filter_map(|(line_index, line)| {
            let cells: Vec<_> = line
                .trim()
                .trim_matches('|')
                .split('|')
                .map(str::trim)
                .collect();
            let label = *cells.first()?;
            if !label.starts_with("ADR-") {
                return None;
            }
            let target = cells
                .get(2)
                .and_then(|cell| MARKDOWN_LINK.captures(cell))
                .and_then(|captures| captures.get(1))
                .map_or_else(String::new, |target| target.as_str().to_owned());
            Some(IndexRow {
                label: label.to_owned(),
                target,
                line: line_index + 1,
            })
        })
        .collect()
}

fn collect_adr_files(root: &Path) -> std::io::Result<Vec<AdrFile>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(root.join(ADR_DIR))? {
        let entry = entry?;
        let filename = entry.file_name().to_string_lossy().into_owned();
        if adr_number(&filename).is_some() {
            files.push(AdrFile {
                contents: fs::read_to_string(entry.path())?,
                filename,
            });
        }
    }
    files.sort_by(|left, right| left.filename.cmp(&right.filename));
    Ok(files)
}

fn validate_index(files: &[AdrFile], rows: &[IndexRow]) -> Vec<String> {
    let mut violations = Vec::new();
    let mut numbers = std::collections::BTreeMap::<u16, Vec<&str>>::new();
    for file in files {
        if let Some(number) = adr_number(&file.filename) {
            if number == 0 {
                violations.push(format!(
                    "ADR file {} uses ADR-000; numbering starts at ADR-001",
                    file.filename
                ));
            }
            numbers.entry(number).or_default().push(&file.filename);
        }
    }

    for (number, filenames) in &numbers {
        if filenames.len() > 1 {
            violations.push(format!(
                "ADR-{number:03} has duplicate files: {}",
                filenames.join(", ")
            ));
        }
    }

    if let Some(highest) = numbers.keys().next_back() {
        for number in 1..=*highest {
            if number != ABSENT_ADR_NUMBER && !numbers.contains_key(&number) {
                violations.push(format!("ADR-{number:03} is a numbering gap with no file"));
            }
        }
    }

    for file in files {
        let matching_rows: Vec<_> = rows
            .iter()
            .filter(|row| {
                resolve_adr_target(&row.target).as_deref() == Some(file.filename.as_str())
            })
            .collect();
        match matching_rows.len() {
            0 => violations.push(format!("ADR file {} has no index row", file.filename)),
            1 => {}
            count => violations.push(format!("ADR file {} has {count} index rows", file.filename)),
        }
    }

    for row in rows {
        let target_filename = resolve_adr_target(&row.target).unwrap_or_default();
        let target_number = adr_number(&target_filename);
        let target_exists = files.iter().any(|file| file.filename == target_filename);
        if !target_exists {
            violations.push(format!(
                "ADR index row {} at line {} points to missing file {}",
                row.label,
                row.line,
                if row.target.is_empty() {
                    "<no link>"
                } else {
                    &row.target
                }
            ));
        }
        if let Some(number) = target_number
            && row.label != format!("ADR-{number:03}")
        {
            violations.push(format!(
                "ADR index row {} at line {} labels file {} with the wrong number",
                row.label, row.line, target_filename
            ));
        }
    }

    for file in files {
        let Some(own_number) = adr_number(&file.filename) else {
            continue;
        };
        if let Some(status) = section(&file.contents, "Status") {
            for line in status.lines() {
                let links: Vec<_> = STATUS_ADR_LINK.captures_iter(line).collect();
                for link in &links {
                    let number = link[1].parse::<u16>().unwrap_or_default();
                    let target = resolve_adr_target(&link[2]).unwrap_or_default();
                    if !numbers.contains_key(&number) {
                        violations.push(format!(
                            "ADR file {} Status references missing ADR-{number:03}",
                            file.filename
                        ));
                    } else if adr_number(&target) != Some(number)
                        || !files.iter().any(|candidate| candidate.filename == target)
                    {
                        violations.push(format!(
                            "ADR file {} Status link for ADR-{number:03} points to {}",
                            file.filename, target
                        ));
                    }
                }

                let plain_line = STATUS_ADR_LINK.replace_all(line, "");
                for reference in ADR_NUMBER.captures_iter(&plain_line) {
                    let number = reference[1].parse::<u16>().unwrap_or_default();
                    if !numbers.contains_key(&number) {
                        violations.push(format!(
                            "ADR file {} Status references missing ADR-{number:03}",
                            file.filename
                        ));
                    }
                }
            }
        }
        if !numbers.contains_key(&own_number) {
            violations.push(format!(
                "ADR file {} has an invalid filename number",
                file.filename
            ));
        }
    }

    violations
}

fn section<'a>(contents: &'a str, title: &str) -> Option<&'a str> {
    let heading = format!("## {title}");
    let start = contents.find(&heading)? + heading.len();
    let rest = &contents[start..];
    let end = rest.find("\n## ").unwrap_or(rest.len());
    Some(rest[..end].trim())
}

fn row(label: &str, target: &str) -> IndexRow {
    IndexRow {
        label: label.to_owned(),
        target: target.to_owned(),
        line: 1,
    }
}

fn fixture_file(number: u16, status: &str) -> AdrFile {
    AdrFile {
        filename: format!("adr-{number:03}-fixture.md"),
        contents: format!("## Status\n\n{status}\n\n## Decision\n"),
    }
}

#[test]
fn adr_files_and_index_rows_form_a_complete_contract() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let files = collect_adr_files(root)?;
    let index = fs::read_to_string(root.join(INDEX_PATH))?;
    let rows = parse_index(&index);
    let violations = validate_index(&files, &rows);

    assert!(
        violations.is_empty(),
        "ADR index contract violations:\n{}",
        violations.join("\n")
    );
    Ok(())
}

#[test]
fn adr_file_without_an_index_row_is_reported() {
    let violations = validate_index(&[fixture_file(1, "Accepted")], &[]);
    assert!(
        violations
            .iter()
            .any(|issue| issue.contains("has no index row"))
    );
}

#[test]
fn adr_file_with_multiple_index_rows_is_reported() {
    let file = fixture_file(1, "Accepted");
    let rows = [
        row("ADR-001", "adr-001-fixture.md"),
        row("ADR-001", "adr-001-fixture.md"),
    ];
    let violations = validate_index(&[file], &rows);
    assert!(
        violations
            .iter()
            .any(|issue| issue.contains("2 index rows"))
    );
}

#[test]
fn normalized_relative_index_links_resolve_to_the_adr_file() {
    let file = fixture_file(1, "Accepted");
    let rows = [row("ADR-001", "./adr-001-fixture.md")];
    assert!(validate_index(&[file], &rows).is_empty());
}

#[test]
fn index_row_without_an_adr_file_is_reported() {
    let violations = validate_index(&[], &[row("ADR-001", "adr-001-missing.md")]);
    assert!(
        violations
            .iter()
            .any(|issue| issue.contains("points to missing file"))
    );
}

#[test]
fn duplicate_adr_file_numbers_are_reported() {
    let files = [
        AdrFile {
            filename: "adr-001-first.md".into(),
            contents: String::new(),
        },
        AdrFile {
            filename: "adr-001-second.md".into(),
            contents: String::new(),
        },
    ];
    let violations = validate_index(&files, &[]);
    assert!(
        violations
            .iter()
            .any(|issue| issue.contains("duplicate files"))
    );
}

#[test]
fn numbering_gaps_are_reported_except_for_adr_009() {
    let files: Vec<_> = (1..=7)
        .map(|number| fixture_file(number, "Accepted"))
        .chain([fixture_file(10, "Accepted")])
        .collect();
    let violations = validate_index(&files, &[]);
    assert!(
        !violations
            .iter()
            .any(|issue| issue.contains("ADR-009 is a numbering gap"))
    );
    assert!(
        violations
            .iter()
            .any(|issue| issue.contains("ADR-008 is a numbering gap"))
    );
}

#[test]
fn adr_numbering_must_start_at_001() {
    let file = fixture_file(0, "Accepted");
    let rows = [row("ADR-000", "adr-000-fixture.md")];
    let violations = validate_index(&[file], &rows);
    assert!(
        violations
            .iter()
            .any(|issue| issue.contains("numbering starts at ADR-001"))
    );
}

#[test]
fn index_label_must_match_the_linked_filename_number() {
    let file = fixture_file(1, "Accepted");
    let violations = validate_index(&[file], &[row("ADR-002", "adr-001-fixture.md")]);
    assert!(
        violations
            .iter()
            .any(|issue| issue.contains("labels file adr-001-fixture.md"))
    );
}

#[test]
fn missing_status_numbers_are_reported() {
    let file = fixture_file(1, "Superseded by ADR-002");
    let violations = validate_index(&[file], &[]);
    assert!(
        violations
            .iter()
            .any(|issue| issue.contains("Status references missing ADR-002"))
    );
}

#[test]
fn status_links_must_resolve_to_their_adr_number() {
    let file = fixture_file(1, "Superseded by [ADR-002](adr-003-other.md)");
    let second = fixture_file(2, "Accepted");
    let other = fixture_file(3, "Accepted");
    let violations = validate_index(&[file, second, other], &[]);
    assert!(
        violations
            .iter()
            .any(|issue| issue.contains("Status link for ADR-002"))
    );
}
