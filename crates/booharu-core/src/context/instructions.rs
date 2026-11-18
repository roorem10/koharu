//! A glossary written into the translation Instructions, read out of them
//! line by line for Import from instructions, as Booharu 0.0.4 read it.

use serde::{Deserialize, Serialize};

/// A term on a line of the Instructions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct InstructionTerm {
    /// The line it's on, counting from 0, as `str::lines` counts them.
    pub line: u32,
    pub source: String,
    pub target: String,
}

/// The terms the lines of `instructions` write, each with its line: `JP=EN`,
/// `JP = EN`, or `JP → EN` anywhere, and `JP: EN` below a line naming a
/// glossary, or anywhere when its source isn't ASCII, so `Tone: informal`
/// stays an instruction. A line's leading `-` or `*` is a list's bullet. The
/// line naming the glossary is no term itself, and neither is a line whose
/// source or target is blank.
pub fn instruction_terms(instructions: &str) -> Vec<InstructionTerm> {
    let mut glossary = false;
    let mut terms = Vec::new();
    for (index, line) in instructions.lines().enumerate() {
        let text = line.trim().trim_start_matches(['-', '*']).trim();
        if text.to_lowercase().contains("glossary") {
            glossary = true;
            continue;
        }
        let pair = text
            .split_once('→')
            .or_else(|| text.split_once('='))
            .or_else(|| {
                text.split_once(':')
                    .filter(|(source, _)| glossary || !source.is_ascii())
            });
        let Some((source, target)) = pair else {
            continue;
        };
        let (source, target) = (source.trim(), target.trim());
        if !source.is_empty() && !target.is_empty() && !source.contains(['=', '→']) {
            terms.push(InstructionTerm {
                line: index as u32,
                source: source.to_owned(),
                target: target.to_owned(),
            });
        }
    }
    terms
}

#[cfg(test)]
mod tests {
    use super::*;

    fn read(instructions: &str) -> Vec<(u32, String, String)> {
        instruction_terms(instructions)
            .into_iter()
            .map(|term| (term.line, term.source, term.target))
            .collect()
    }

    fn terms(terms: &[(u32, &str, &str)]) -> Vec<(u32, String, String)> {
        terms
            .iter()
            .map(|&(line, source, target)| (line, source.to_owned(), target.to_owned()))
            .collect()
    }

    #[test]
    fn the_four_forms_are_read_with_their_lines_and_the_rest_skipped() {
        let instructions = "Use natural English.\nTone: informal\nヒナ=Hina\n魔王 = Demon King\n\n\
                            - 村 → Village\n町: Town\nKeep honorifics.\n= Nothing\nSensei =";
        assert_eq!(
            read(instructions),
            terms(&[
                (2, "ヒナ", "Hina"),
                (3, "魔王", "Demon King"),
                (5, "村", "Village"),
                (6, "町", "Town"),
            ])
        );
    }

    #[test]
    fn a_colon_below_a_glossary_line_writes_a_term() {
        assert!(read("Tone: informal").is_empty());
        assert_eq!(read("ヒナ: Hina"), terms(&[(0, "ヒナ", "Hina")]));
        assert_eq!(
            read("Rules: none\n## Glossary\n* Sensei: Teacher\nKivotos: Kivotos"),
            terms(&[(2, "Sensei", "Teacher"), (3, "Kivotos", "Kivotos")])
        );
        // 0.0.4's own example.
        let terms = instruction_terms(
            "Use natural English.\nGlossary:\nヒナ=Hina\n魔王 = Demon King\n\n村 → Village\n町: Town",
        );
        assert_eq!(terms.len(), 4);
        assert_eq!((terms[0].line, &*terms[0].source), (2, "ヒナ"));
    }
}
