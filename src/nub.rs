use rkyv::{ rancor, Archive };
use reqwest::Client;
use tantivy::{
    collector::TopDocs,
    doc,
    query::QueryParser,
    schema::{ self, Value },
    DocAddress,
    Index,
    IndexWriter,
    Score,
    TantivyDocument,
};

const NUB_ENDPOINT: &str = "https://solanapulseserver-production.up.railway.app/memeslist";

#[derive(Debug, Archive, rkyv::Serialize, rkyv::Deserialize, serde::Deserialize)]
pub struct Nub {
    pub source: Box<str>,
    pub tags: Vec<Box<str>>,
}

pub async fn fetch_nubs() -> Result<Vec<Nub>, Box<dyn core::error::Error + Send + Sync>> {
    let client = Client::new();
    Ok(client.get(NUB_ENDPOINT).send().await?.json::<Vec<Nub>>().await?)
}

pub fn save_nubs(nubs: &Vec<Nub>) -> Result<(), Box<dyn core::error::Error + Send + Sync>> {
    std::fs::write("four.bin", rkyv::to_bytes::<rancor::Error>(nubs)?)?;
    Ok(())
}

pub fn load_nubs() -> Result<Vec<Nub>, Box<dyn core::error::Error + Send + Sync>> {
    Ok(rkyv::from_bytes::<Vec<Nub>, rancor::Error>(&std::fs::read("four.bin")?)?)
}

pub async fn get_nubs() -> Result<Vec<Nub>, Box<dyn core::error::Error + Send + Sync>> {
    if std::fs::exists("four.bin")? {
        load_nubs()
    } else {
        let nubs = fetch_nubs().await?;
        save_nubs(&nubs)?;
        Ok(nubs)
    }
}

pub struct NubFinder {
    index: Index,
    fields: (schema::Field, schema::Field),
}

impl NubFinder {
    pub fn new() -> Result<Self, Box<dyn core::error::Error + Send + Sync>> {
        let mut builder = schema::Schema::builder();
        let field_url = builder.add_text_field("url", schema::STRING | schema::STORED);
        let field_keywords = builder.add_text_field("keywords", schema::TEXT | schema::STORED);
        let schema = builder.build();

        let index = Index::create_in_ram(schema.clone());

        Ok(Self { index, fields: (field_url, field_keywords) })
    }

    pub fn commit(&self, nubs: Vec<Nub>) -> Result<(), Box<dyn core::error::Error + Send + Sync>> {
        let mut index_writer: IndexWriter = self.index.writer(100_000_000)?;
        for nub in nubs {
            let source = nub.source.as_bytes();
            let keywords_str = nub.tags.join(", ");

            index_writer.add_document(
                doc!(
                    self.fields.0 => source,
                    self.fields.1 => keywords_str
                )
            )?;
        }
        index_writer.commit()?;

        Ok(())
    }

    /// Returns: `(url, keywords)`
    pub fn search(
        &self,
        q: &str
    ) -> Result<Vec<(String, String)>, Box<dyn core::error::Error + Send + Sync>> {
        let reader = self.index.reader()?;
        let searcher = reader.searcher();
        let query_parser = QueryParser::for_index(&self.index, vec![self.fields.0, self.fields.1]);
        let query = query_parser.parse_query(q)?;

        let top_docs: Vec<(Score, DocAddress)> = searcher.search(&query, &TopDocs::with_limit(10))?;
        let mut results = Vec::new();

        for (_score, doc_address) in top_docs {
            let doc = searcher.doc::<TantivyDocument>(doc_address)?;
            if let Some(url_value) = doc.get_first(self.fields.0) {
                let value = url_value.as_value();
                let url = String::from_utf8_lossy(value.as_bytes().unwrap()).into_owned();

                if let Some(kw_value) = doc.get_first(self.fields.1) {
                    let value = kw_value.as_value();
                    let kw = value.as_str().unwrap().to_owned();

                    results.push((url, kw));
                }
            }
        }
        Ok(results)
    }
}
