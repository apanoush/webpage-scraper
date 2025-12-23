use headless_chrome;
use anyhow;
use url::{Url, ParseError};
use thiserror::Error;
use crate::webpage::{WebPage, WebPageError};

#[derive(Error, Debug)]
pub enum BrowserError {
    #[error("ChromeError: {0}")]
    ChromeError(#[from] anyhow::Error),
    #[error("UrlError, can't parse given URL: {0}")]
    UrlError(#[from] ParseError),
    #[error("WebPageError: {0}")]
    WebPageError(#[from] WebPageError)
}
pub type Result<T> = std::result::Result<T, BrowserError>;

pub struct Browser (headless_chrome::Browser);

impl Browser {
    
    fn new() -> Result<Self> {
        Ok(Self(headless_chrome::Browser::default()?))
    }
    async fn open_tab(&self, url: &str) -> Result<WebPage> {
    
        Url::parse(url)?;
        let tab = self.0.new_tab()?;

        tab.navigate_to(url)?;
        tab.wait_until_navigated()?;

        let webpage = WebPage::from_tab(tab).await?;

        Ok(webpage)

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
