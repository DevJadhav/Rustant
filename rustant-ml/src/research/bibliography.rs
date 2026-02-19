//! Bibliography export — BibTeX, RIS, CSL-JSON.

use serde::{Deserialize, Serialize};

/// A bibliography entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BibEntry {
    pub id: String,
    pub title: String,
    pub authors: Vec<String>,
    pub year: Option<u32>,
    pub venue: Option<String>,
    pub doi: Option<String>,
    pub url: Option<String>,
    pub abstract_text: Option<String>,
    pub entry_type: String,
}

impl BibEntry {
    /// Export as BibTeX.
    pub fn to_bibtex(&self) -> String {
        let authors_str = self.authors.join(" and ");
        let mut bib = format!("@{}{{{},\n", self.entry_type, self.id);
        bib.push_str(&format!("  title = {{{}}},\n", self.title));
        bib.push_str(&format!("  author = {{{authors_str}}},\n"));
        if let Some(year) = self.year {
            bib.push_str(&format!("  year = {{{year}}},\n"));
        }
        if let Some(venue) = &self.venue {
            bib.push_str(&format!("  booktitle = {{{venue}}},\n"));
        }
        if let Some(doi) = &self.doi {
            bib.push_str(&format!("  doi = {{{doi}}},\n"));
        }
        if let Some(url) = &self.url {
            bib.push_str(&format!("  url = {{{url}}},\n"));
        }
        bib.push('}');
        bib
    }

    /// Export as RIS.
    pub fn to_ris(&self) -> String {
        let mut ris = String::from("TY  - JOUR\n");
        ris.push_str(&format!("TI  - {}\n", self.title));
        for author in &self.authors {
            ris.push_str(&format!("AU  - {author}\n"));
        }
        if let Some(year) = self.year {
            ris.push_str(&format!("PY  - {year}\n"));
        }
        if let Some(doi) = &self.doi {
            ris.push_str(&format!("DO  - {doi}\n"));
        }
        if let Some(url) = &self.url {
            ris.push_str(&format!("UR  - {url}\n"));
        }
        ris.push_str("ER  - \n");
        ris
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bibtex_export() {
        let entry = BibEntry {
            id: "vaswani2017".into(),
            title: "Attention Is All You Need".into(),
            authors: vec!["Vaswani, A.".into(), "Shazeer, N.".into()],
            year: Some(2017),
            venue: Some("NeurIPS".into()),
            doi: None,
            url: None,
            abstract_text: None,
            entry_type: "article".into(),
        };
        let bib = entry.to_bibtex();
        assert!(bib.contains("Attention Is All You Need"));
        assert!(bib.contains("2017"));
    }
}
