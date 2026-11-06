use std::collections::BTreeMap;
use step_link::parser::parse_part21;

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage : inspect <fichier.step>");
        return;
    };
    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => {
            eprintln!("{path} : {e}");
            return;
        }
    };
    let file = match parse_part21(&text) {
        Ok(file) => file,
        Err(e) => {
            eprintln!("erreur : {e}");
            return;
        }
    };

    println!(
        "{} enregistrements d'en-tête, {} entités",
        file.header.len(),
        file.entities.len()
    );

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for entity in file.entities.values() {
        for record in &entity.records {
            *counts.entry(record.name.clone()).or_default() += 1;
        }
    }
    let mut sorted: Vec<_> = counts.into_iter().collect();
    sorted.sort_by(|a, b| b.1.cmp(&a.1));
    for (name, count) in sorted {
        println!("{count:>6}  {name}");
    }
}