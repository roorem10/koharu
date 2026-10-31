use std::collections::HashSet;

use anyhow::{Result, bail, ensure};

pub(super) fn validate_pattern(pattern: &str) -> Result<()> {
    ensure!(
        !pattern.trim().is_empty(),
        "export name pattern cannot be empty"
    );
    ensure!(pattern.len() <= 4096, "export name pattern is too long");
    let mut rest = pattern;
    while let Some(start) = rest.find(['{', '}']) {
        ensure!(
            rest.as_bytes()[start] == b'{',
            "unmatched closing brace in export name pattern"
        );
        let token = &rest[start + 1..];
        let end = token
            .find('}')
            .ok_or_else(|| anyhow::anyhow!("unclosed export name token"))?;
        match &token[..end] {
            "project" | "index" | "number" | "page" => {}
            token => bail!(
                "unknown export name token {{{token}}}; use {{project}}, {{index}}, {{number}} or {{page}}"
            ),
        }
        rest = &token[end + 1..];
    }
    Ok(())
}

fn safe_stem(value: &str) -> String {
    let value = value.replace(|c: char| c.is_control() || "<>:\"/\\|?*".contains(c), "_");
    let value = value.trim().trim_end_matches(['.', ' ']);
    let value = if value.is_empty() { "page" } else { value };
    let device = value
        .split('.')
        .next()
        .unwrap_or(value)
        .to_ascii_uppercase();
    if matches!(device.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (device.len() == 4
            && (device.starts_with("COM") || device.starts_with("LPT"))
            && matches!(device.as_bytes()[3], b'1'..=b'9'))
    {
        format!("_{value}")
    } else {
        value.to_owned()
    }
}

pub(crate) fn export_names(
    pattern: &str,
    project: &str,
    labels: &[&str],
    extension: &str,
) -> Result<Vec<String>> {
    validate_pattern(pattern)?;
    let width = labels.len().to_string().len().max(4);
    let mut used = HashSet::new();
    labels
        .iter()
        .enumerate()
        .map(|(index, label)| {
            let label = label
                .trim()
                .trim_end_matches(|c: char| c == '.' || c.is_whitespace());
            let page = label.rsplit_once('.').map_or(label, |(stem, _)| stem);
            let page = if page.is_empty() { "page" } else { page };
            // Substitute tokens in a single pass so braces in a label stay literal.
            let mut rest = pattern;
            let mut name = String::new();
            while let Some(start) = rest.find('{') {
                name.push_str(&rest[..start]);
                let end = rest[start..].find('}').expect("validated token") + start;
                name.push_str(&match &rest[start + 1..end] {
                    "project" => project.to_owned(),
                    "index" => format!("{:0width$}", index + 1),
                    "number" => (index + 1).to_string(),
                    "page" => page.to_owned(),
                    _ => unreachable!("validated token"),
                });
                rest = &rest[end + 1..];
            }
            name.push_str(rest);
            let stem = safe_stem(&name);
            let mut unique = stem.clone();
            let mut copy = 2;
            while !used.insert(unique.to_lowercase()) {
                unique = format!("{stem} ({copy})");
                copy += 1;
            }
            Ok(format!("{unique}.{extension}"))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_padding_and_safe_names() {
        assert_eq!(
            export_names(
                "{project}-{index}-{number}-{page}",
                "A:B",
                &["a/b.png"],
                "psd"
            )
            .unwrap(),
            ["A_B-0001-1-a_b.psd"]
        );
        assert_eq!(
            export_names("{index}_{page}", "x", &[".png", " a?.png. "], "png").unwrap(),
            ["0001_page.png", "0002_a_.png"]
        );
        assert_eq!(
            export_names("{page}", "x", &["CON.png", "../.png"], "png").unwrap(),
            ["_CON.png", ".._.png"]
        );
        assert!(
            export_names("{index}", "x", &vec!["p"; 10000], "png").unwrap()[0].starts_with("00001")
        );
    }

    #[test]
    fn rejects_unknown_and_unbalanced_tokens() {
        for pattern in ["{filename}", "{index", "index}", "", "{x{page}"] {
            assert!(validate_pattern(pattern).is_err(), "{pattern}");
        }
    }

    #[test]
    fn deduplicates_case_insensitively_including_suffix_collisions() {
        assert_eq!(
            export_names(
                "{page}",
                "x",
                &["A.png", "a.png", "A (2).png", "A.png"],
                "png"
            )
            .unwrap(),
            ["A.png", "a (2).png", "A (2) (2).png", "A (3).png"]
        );
        assert_eq!(
            export_names("{page}", "x", &["{number}.png"], "png").unwrap(),
            ["{number}.png"]
        );
    }
}
