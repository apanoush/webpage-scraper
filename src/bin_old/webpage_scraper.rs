use webpage_scraper::browser;
use clap::Parser;
use tokio;

/// Scraps a website, HTML (and its pandoc Markdown conversion), 
/// info JSON and images
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// URL of the webpage to be scraped
    url: String,

    /// Name of the output_directory
    /// if not given, will use the name of the website
    output_directory: Option<String>
}

#[tokio::main]
async fn main() {

    let args = Args::parse();

    let browser = browser::Browser::new().expect("Can't initiate browser");

    let webpage = browser.open_tab(&args.url).await.unwrap();

    let output_directory = match args.output_directory {
        Some(e) => e,
        None => webpage.title.clone()
    };

    webpage.write_to_disk(&output_directory).await.expect("Can't write scraped data to disk");

}
