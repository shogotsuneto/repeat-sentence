// Built-in practice sentences and the parser for user-imported text.

/// Original sentences in the style of PTE Repeat Sentence items: short
/// academic / campus-life statements of roughly 3–9 seconds when read aloud.
pub const BUILTIN: &[&str] = &[
    "The library will be closed on Sunday for scheduled maintenance.",
    "Students must submit their assignments before the end of the week.",
    "The lecture on climate change has been moved to the main auditorium.",
    "Please make sure you register for the course by Friday afternoon.",
    "Most of the research data was collected during the summer months.",
    "The professor asked us to read the first three chapters before class.",
    "Our study group meets every Tuesday in the student union building.",
    "Economic growth depends heavily on investment in education and infrastructure.",
    "The results of the experiment were published in a leading scientific journal.",
    "You can borrow up to ten books at a time with your student card.",
    "The deadline for scholarship applications has been extended by two weeks.",
    "Renewable energy sources are becoming increasingly affordable around the world.",
    "All first-year students are required to attend the orientation session.",
    "The museum offers free guided tours on the first Monday of every month.",
    "Critical thinking is one of the most valuable skills you can develop at university.",
    "The seminar will focus on the social impact of new technologies.",
    "Please switch off your mobile phones before the examination begins.",
    "Urban populations have grown rapidly over the last few decades.",
    "The tutor will provide detailed feedback on your first draft.",
    "Many animals migrate long distances to find food and suitable breeding grounds.",
    "The cafeteria on the second floor is open until eight in the evening.",
    "Accurate referencing is essential to avoid any accusation of plagiarism.",
    "Language is constantly evolving to reflect changes in society.",
    "The survey revealed that most participants preferred online learning.",
    "Laboratory safety glasses must be worn at all times.",
    "The history department is looking for volunteers to help with the conference.",
    "Regular exercise has a positive effect on both physical and mental health.",
    "Ancient civilisations developed sophisticated systems of agriculture and trade.",
    "Your final grade will be based on two essays and a written exam.",
    "Parking permits for the new semester can be purchased online.",
    "The government introduced new policies to reduce air pollution in major cities.",
    "Please check the noticeboard regularly for changes to the timetable.",
    "Water scarcity is expected to become a serious global issue.",
    "The guest speaker has worked in international development for over twenty years.",
    "Group presentations will take place during the last week of term.",
    "Most of the world's population now lives in urban areas.",
    "The computer lab is available to students twenty-four hours a day.",
    "Effective communication is the key to successful teamwork.",
    "The study found a strong link between sleep and academic performance.",
    "Late submissions will lose five percent of the mark for each day.",
    "Marine biologists are studying the effects of rising ocean temperatures on coral reefs.",
    "Tickets for the graduation ceremony must be collected from the main office.",
    "Photosynthesis is the process by which plants convert sunlight into energy.",
    "The art gallery is hosting an exhibition of contemporary sculpture.",
    "Consumers are increasingly concerned about where their food comes from.",
    "You should discuss your research topic with your supervisor as soon as possible.",
    "The population of the city doubled in less than twenty years.",
    "Several departments have joined forces to launch a new interdisciplinary program.",
];

/// One sentence per line. Blank lines and `#` comments are skipped.
pub fn parse_lines(content: &str) -> Vec<String> {
    dedupe(
        strip_bom(content)
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .map(normalize),
    )
}

fn strip_bom(s: &str) -> &str {
    s.strip_prefix('\u{feff}').unwrap_or(s)
}

/// Collapses internal runs of whitespace into single spaces.
fn normalize(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn dedupe(items: impl Iterator<Item = String>) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    items.filter(|s| seen.insert(s.clone())).collect()
}

/// File name without directory or extension, used as the imported set name.
pub fn set_name_from_file(file_name: &str) -> String {
    let base = file_name.rsplit(['/', '\\']).next().unwrap_or(file_name);
    match base.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem.to_string(),
        _ => base.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_has_dozens_of_unique_sentences() {
        assert!(BUILTIN.len() >= 30);
        assert_eq!(
            dedupe(BUILTIN.iter().map(|s| s.to_string())).len(),
            BUILTIN.len()
        );
    }

    #[test]
    fn lines_skip_blanks_comments_and_duplicates() {
        let txt = "\u{feff}First one.\r\n\n  # comment\n  Second   one. \nFirst one.\n";
        assert_eq!(parse_lines(txt), vec!["First one.", "Second one."]);
    }

    #[test]
    fn example_file_parses() {
        let sentences = parse_lines(include_str!("../examples/everyday.txt"));
        assert_eq!(sentences.len(), 28);
        assert!(sentences.iter().all(|s| !s.starts_with('#')));
        assert!(sentences.iter().all(|s| !BUILTIN.contains(&s.as_str())));
    }

    #[test]
    fn set_names() {
        assert_eq!(set_name_from_file("week1.txt"), "week1");
        assert_eq!(set_name_from_file("dir/my.list.txt"), "my.list");
        assert_eq!(set_name_from_file(".hidden"), ".hidden");
        assert_eq!(set_name_from_file("noext"), "noext");
    }
}
