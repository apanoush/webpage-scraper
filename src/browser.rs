use headless_chrome;
use anyhow;
use url::{Url, ParseError};
use thiserror::Error;
use crate::webpage::{WebPage, WebPageError};
use std::path::Path;
use std::sync::Arc;

#[derive(Error, Debug)]
pub enum BrowserError {
    #[error("ChromeError: {0}")]
    ChromeError(#[from] anyhow::Error),
    #[error("UrlError, can't parse given URL: {0}")]
    UrlError(#[from] ParseError),
    #[error("WebPageError: {0}")]
    WebPageError(#[from] WebPageError),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error)
}
pub type Result<T> = std::result::Result<T, BrowserError>;

pub struct Browser (headless_chrome::Browser);

impl Browser {
    
    pub fn new() -> Result<Self> {
        Ok(Self(headless_chrome::Browser::default()?))
    }

    fn url_to_tab(&self, url: &str) -> Result<Arc<headless_chrome::Tab>> {
        
        Url::parse(url)?;
        let tab = self.0.new_tab()?;

        tab.navigate_to(url)?.wait_until_navigated()?;

        Ok(tab)

    }

    pub async fn open_tab(&self, url: &str) -> Result<WebPage> {
    
        let tab = self.url_to_tab(url)?;

        let webpage = WebPage::from_tab(tab).await?;

        Ok(webpage)
    }

    pub fn url_to_pdf(&self, url: &str) -> Result<()> {

        let tab = self.url_to_tab(url)?;
        let title = tab.get_title()?;
        let filename = format!("{}.pdf", title);
        let output_path = Path::new(&filename);
        let pdf = tab.print_to_pdf(None)?;
        std::fs::write(&output_path, pdf)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_complicated_website() {
        let b = Browser::new().unwrap();
        let link = "https://100-beste-plakate.de/plakate/";
        //let link = "https://en.wikipedia.org/wiki/%C3%89cole_cantonale_d%27art_de_Lausanne";
        let tab = b.open_tab(link).await.unwrap();

        tab.write_to_disk("test/complicated_website").await.unwrap();
    }
}
