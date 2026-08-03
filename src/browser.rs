use headless_chrome;
use anyhow;
use url::{Url, ParseError};
use thiserror::Error;
use crate::webpage::{WebPage, WebPageError};
use std::path::Path;
use std::sync::Arc;
use serde_json;

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

        tab.evaluate(
            "new Promise((resolve) => {
                const getH = () => Math.max(
                    document.body.scrollHeight,
                    document.body.offsetHeight,
                    document.documentElement.scrollHeight,
                    document.documentElement.offsetHeight
                );
                let lastH = getH();
                let stable = 0;
                const maxCycles = 100;
                let cycles = 0;
                const timer = setInterval(() => {
                    window.scrollBy(0, 300);
                    cycles++;
                    setTimeout(() => {
                        const h = getH();
                        const atBottom = window.innerHeight + window.scrollY >= h - 50;
                        if (h === lastH && atBottom) { stable++; } else { stable = 0; lastH = h; }
                        if ((stable >= 3 && atBottom) || cycles >= maxCycles) {
                            clearInterval(timer);
                            window.scrollTo(0, 0);
                            resolve();
                        }
                    }, 150);
                }, 250);
            })",
            true,
        )?;

        tab.evaluate(
            "new Promise((resolve) => {
                let lastLength = document.body.innerHTML.length;
                let stable = 0;
                const neededStable = document.querySelector('.loader, .upt-loader, .loading, [data-loading], [aria-busy=\"true\"], .spinner') ? 3 : 2;
                const check = setInterval(() => {
                    const len = document.body.innerHTML.length;
                    const busy = document.querySelector('.loader, .upt-loader, .loading, [data-loading], [aria-busy=\"true\"], .spinner');
                    if (len === lastLength && !busy) {
                        stable++;
                        if (stable >= neededStable) {
                            clearInterval(check);
                            resolve();
                        }
                    } else {
                        stable = 0;
                        lastLength = len;
                    }
                }, 1000);
            })",
            true,
        )?;

        Ok(tab)

    }

    pub async fn open_tab(&self, url: &str, no_conversions: bool, download_videos: bool) -> Result<WebPage> {
    
        let tab = self.url_to_tab(url)?;

        let css_urls: Vec<String> = tab.evaluate(
            "JSON.stringify([...new Set(
                performance.getEntriesByType('resource')
                    .filter(e => e.initiatorType === 'css' && e.name && e.name.includes('.css'))
                    .map(e => e.name)
            )])",
            false,
        )?
        .value
        .and_then(|v| v.as_str().map(String::from))
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default();

        let webpage = WebPage::from_tab(tab, no_conversions, download_videos, css_urls).await?;

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
        let tab = b.open_tab(link, false, false).await.unwrap();

        tab.write_to_disk("test/complicated_website", false).await.unwrap();
    }
}
