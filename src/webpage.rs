use std::{fs, path::{Path, PathBuf}};
use pandoc;
use time::OffsetDateTime;
use thiserror::Error;
use std::sync::Arc;
use headless_chrome;
use anyhow;
use futures::future;
use serde_json;
use serde::Serialize;
use crate::images::{Images, ImagesError};
use scraper::{Html, Selector};

pub struct WebPage {
    url: String,
    title: String,
    date: String,
    html: String,
    images: Images,
    markdown: String,
    tab: Arc<headless_chrome::Tab>,
    info_json: InfoJson
}

#[derive(Serialize)]
pub struct InfoJson {
    url: String,
    title: String,
    date: String,
    nb_md_words: usize,
    nb_images: usize,
}

#[derive(Error, Debug)]
pub enum WebPageError {
    #[error("I/O Error: {0}")]
    IO(#[from] std::io::Error),
    #[error("MarkdownConversion error: {0}")]
    MarkdownConversionError(#[from] pandoc::PandocError),
    #[error("Task failed: {0}")]
    TaskFailed(#[from] tokio::task::JoinError),
    #[error("Time error: {0}")]
    TimeError(#[from] time::error::IndeterminateOffset),
    #[error("ImagesError: {0}")]
    ImagesError(#[from] ImagesError),
    #[error("AnyhowError: {0}")]
    AnyhowError(#[from] anyhow::Error),
    #[error("JSON conversion error: {0}")]
    JsonConversionError(#[from] serde_json::Error)
}

pub type Result<T> = std::result::Result<T, WebPageError>;

impl WebPage {

    pub async fn from_tab(tab: Arc<headless_chrome::Tab>) -> Result<Self> {

        let today = OffsetDateTime::now_local()?.date().to_string();

        let title = tab.get_title()?;
        let url = tab.get_url();
        let html = tab.get_content()?;

        let md = WebPage::html2md(html.clone());
        let images = Images::from(&html, &url);

        let (md, images) = future::join(md, images).await;

        let md = md?; let images = images?;

        let nb_md_words = md.split_whitespace().count();
        let nb_images = images.len();
       
        let info_json = InfoJson {
            url: url.clone(), title: title.clone(), date: today.clone(), nb_md_words: nb_md_words, nb_images: nb_images,
        };

        Ok( Self {
            url: url,
            title: title,
            date: today,
            markdown: md,
            images: images,
            html: html,
            tab: tab,
            info_json: info_json
        })


    }

    fn html_to_simple_markdown(html: &str) -> String {
        let fragment = Html::parse_fragment(html);
        let body = fragment.root_element();

        let mut markdown = String::new();

        // Define selectors for headers and text-containing elements
        let header_selector = Selector::parse("h1, h2, h3, h4, h5, h6").unwrap();
        let text_selector = Selector::parse("p, div, span, article, section, main").unwrap();

        // Collect all nodes to process in document order
        let nodes: Vec<scraper::ElementRef> = body
            .select(&header_selector)
            .chain(body.select(&text_selector))
            //.filter(|el| !is_descendant_of_header(el)) // avoid nested text inside headers
            .collect();

        // Sort by document order (scraper select already yields in doc order)
        for el in nodes {
            let tag_name = el.value().name();
            let text = el.text().collect::<Vec<_>>().join(" ").trim().to_string();

            if text.is_empty() {
                continue;
            }

            match tag_name {
                "h1" => markdown.push_str(&format!("# {}\n\n", text)),
                "h2" => markdown.push_str(&format!("## {}\n\n", text)),
                "h3" => markdown.push_str(&format!("### {}\n\n", text)),
                "h4" => markdown.push_str(&format!("#### {}\n\n", text)),
                "h5" => markdown.push_str(&format!("##### {}\n\n", text)),
                "h6" => markdown.push_str(&format!("###### {}\n\n", text)),
                _ => markdown.push_str(&format!("{}\n\n", text)), // plain paragraph
            }
        }

        // Optional: clean extra newlines
        markdown.trim().to_string().replace("\n\n\n", "\n\n")
    }

    async fn html2md(html: String) -> Result<String> {
        
        let mut pandoc = pandoc::Pandoc::new();

        pandoc
            .set_input(pandoc::InputKind::Pipe(html))
            .set_input_format(
                pandoc::InputFormat::Html, 
                vec![]
            )
            .set_output(pandoc::OutputKind::Pipe)
            .set_output_format(
                pandoc::OutputFormat::Other("gfm-raw_html".to_string()), 
                vec![]
            );

        let res = pandoc.execute()?;

        match res {
            pandoc::PandocOutput::ToBuffer(e) => return Ok(e),
            _ => return Err(WebPageError::MarkdownConversionError(
                pandoc::PandocError::PandocNotFound
            ))
        }
    }

    pub async fn write_to_disk(&self, output_path: &str) -> Result<()> {

        let output_path = PathBuf::from(output_path);

        if output_path.is_file() || output_path.is_dir() {
            return Err(WebPageError::IO(
                std::io::Error::new(std::io::ErrorKind::InvalidInput, "Output path already exists")
            ));
        }

        std::fs::create_dir(&output_path)?;

        let html_res = self.output_html(output_path.as_path());
        let pdf_res = self.output_pdf(output_path.as_path());
        let md_res = self.output_markdown(output_path.as_path()); 
        let images_res = self.images.write_images_to_disk(output_path.as_path());
        let info_json_res = self.output_info_json(output_path.as_path());

        let (html_res, pdf_res, md_res, images_res, info_json_res) = future::join5(html_res, pdf_res, md_res, images_res, info_json_res).await;

        html_res?; pdf_res?; md_res?; images_res?; info_json_res?;

        Ok(())
    }

    pub async fn output_pdf(&self, output_path: &Path) -> Result<()> {
        let output_path = output_path.join(format!("{}.pdf", self.title));
        let pdf = self.tab.print_to_pdf(None)?;
        std::fs::write(output_path, pdf)?;
        Ok(())
    }

    async fn output_html(&self, output_path: &Path) -> Result<()> {
        let html_path = output_path.join(format!("{}.html", self.title));
        fs::write(html_path, &self.html)?;
        Ok(())
    }

    async fn output_markdown(&self, output_path: &Path) -> Result<()> {
        let output_path = output_path.join(format!("{}.md", self.title));
        fs::write(output_path, &self.markdown)?;
        //println!("Saved markdown to {}", path.display());
        Ok(())
    }
     
    async fn output_info_json(&self, output_path: &Path) -> Result<()> {
        let output_path = output_path.join("informations.json");
        let json = serde_json::to_string_pretty(&self.info_json)?;
        fs::write(output_path, json)?;
        Ok(())
    }

}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_html_epfl() {
        
        let html = std::fs::read_to_string("test/htmls/EPFL.html").unwrap();
        let md = WebPage::html2md(html).await.unwrap();
        //let md = WebPage::html_to_simple_markdown(&html);
        std::fs::write("test/test_markdown/markdown_epfl.md", md).unwrap();
        
    }

    #[tokio::test]
    async fn test_html_ecal() {
        
        let html = std::fs::read_to_string("test/htmls/100 BESTE PLAKATE 24, 17.12.2025–15.01.2026, Galerie l'elac, ECAL - ECAL.html").unwrap();
        let md = WebPage::html2md(html).await.unwrap();
        //let md = WebPage::html_to_simple_markdown(&html);
        std::fs::write("test/test_markdown/markdown_ecal.md", md).unwrap();
        
    }


}
