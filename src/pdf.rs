use headless_chrome;
use std::sync::Arc;
use url::{Url, ParseError};
use thiserror::Error;
use anyhow;

#[derive(Error, Debug)]
pub enum PdfError {
    #[error("UrlError: {0}")]
    UrlError(#[from] ParseError),
    #[error("Browser Error: {0}")]
    BrowserError(#[from] anyhow::Error),
    #[error("IO error: {0}")]
    IOError(#[from] std::io::Error)
}

pub type Result<T> = std::result::Result<T, PdfError>;

pub struct Browser (headless_chrome::Browser);

impl Browser {
    
    fn new() -> Result<Self> {
        Ok(Self(headless_chrome::Browser::default()?))
    }
    fn open_tab(&self, url: &str) -> Result<WebPage> {
    
        Url::parse(url)?;
        let tab = self.0.new_tab()?;

        tab.navigate_to(url)?;
        tab.wait_until_navigated()?;

        Ok(WebPage::from_tab(tab))

    }
}


pub struct WebPage (Arc<headless_chrome::Tab>);


impl WebPage {

    pub fn from_tab(tab: Arc<headless_chrome::Tab>) -> Self {
        Self(tab)
    }


    pub fn to_pdf(&self, path: &str) -> Result<()> {
        let pdf = self.0.print_to_pdf(None)?;
        std::fs::write(path, pdf)?;
        Ok(())
    }

}

#[cfg(test)]
mod tests {
    
    use super::*;

    #[test]
    fn test_tab() {

        let browser = Browser::new().unwrap();

        let wp = browser.open_tab("https://google.com").unwrap();

        let path = "res/google.pdf";

        wp.to_pdf(&path).unwrap();

    }

    #[test]
    fn complicated_pdf() {
    
        let browser = Browser::new().unwrap();
        let wp = browser.open_tab("https://ecal.ch/fr/feed/events/1860/100-beste-plakate-24/").unwrap();

        //wp.to_pdf_with_injections("res/complicated_pdf.pdf").unwrap();
        wp.to_pdf("res/pdf.pdf").unwrap();

    }
}
