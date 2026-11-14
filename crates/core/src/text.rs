//! Utilidades de texto compartidas: palabras clave de una nota.

const STOPWORDS: &[&str] = &[
    "para", "esta", "este", "esto", "estos", "estas", "como", "pero", "tiene", "hace", "hacer", "falta",
    "toca", "porque", "cuando", "donde", "desde", "hasta", "sobre", "entre", "todo", "toda", "todos",
    "algo", "también", "tambien", "solo", "sólo", "debe", "deben", "puede", "pueden", "quiero", "queda",
    "that", "this", "with", "from", "have", "should", "does", "into", "when", "what",
];

/// Palabras útiles de un texto, en minúsculas, sin repetir y en orden de aparición. Se descartan
/// las de menos de 4 letras (el índice de Engram usa trigramas) y las muletillas.
pub fn keywords(text: &str) -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    for w in text.split(|c: char| !c.is_alphanumeric() && c != '_' && c != '-') {
        let w = w.trim_matches('-').to_lowercase();
        if w.chars().count() < 4 || STOPWORDS.contains(&w.as_str()) || words.contains(&w) {
            continue;
        }
        words.push(w);
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn palabras_utiles() {
        assert_eq!(keywords("El título NO carga, el título queda raro"), ["título", "carga", "raro"]);
        assert!(keywords("ok sí de la").is_empty());
    }
}
