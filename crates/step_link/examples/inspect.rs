use std::collections::BTreeMap;
use step_link::brep::Surface;
use step_link::mesh::tessellate;
use step_link::parser::{parse_part21, parse_step};

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

    match parse_step(&text) {
        Ok(model) => {
            for solid in &model.solids {
                let planes = solid
                    .faces()
                    .filter(|f| matches!(f.surface, Surface::Plane { .. }))
                    .count();
                let holes: usize = solid.faces().map(|f| f.inner_loops.len()).sum();
                println!(
                    "solide : {} sommets, {} arêtes, {} faces ({} planes), {} contours internes, {} triangles",
                    solid.vertices.len(),
                    solid.edges.len(),
                    solid.face_count(),
                    planes,
                    holes,
                    tessellate(solid).triangle_count()
                );
            }
        }
        Err(e) => eprintln!("erreur de reconstruction : {e}"),
    }
}