use std::io;
use std::sync::Arc;

use hyper::client::HttpConnector;
use hyper_rustls::HttpsConnector;
use hyper::{Client, Uri, Response, StatusCode, Request, Method, Body};
use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
use sha2::digest::generic_array::GenericArray;
use tokio::fs::{File, self};
use hyper::body::HttpBody;
use tokio::io::{AsyncWriteExt, AsyncReadExt};
use sha2::{Sha256, Digest};
use thiserror::Error;
use hex::FromHex;
use std::path::PathBuf;

const VERSION: &str = "1.0.0alpha";

#[derive(Debug, Error)]
pub enum DownloadError {
    #[error(transparent)]
    Io(#[from] io::Error),

    #[error("failed to open file")]
    FileOpen,

    #[error("url parsing failed")]
    UrlParse,

    #[error("redirect location conversion error")]
    LocationError,

    #[error("Http response error: {0}")]
    HyperError(#[from] hyper::Error),
}

#[derive(Debug, Error)]
pub enum ChecksumError {
    #[error("checksum failed; expected {}, found {}", expected, found)]
    Checksum { expected: String, found: String },

    #[error("expected checksum isn't a valid checksum")]
    InvalidInput,

    #[error("File read error: {0}")]
    ReadFile(#[from] std::io::Error),

    #[error("Invalid hex conversion: {0}")]
    HexFromError(#[from] hex::FromHexError),
}

pub async fn file_exists(file_path: &str) -> bool {
    let metadata = fs::metadata(file_path).await;

    metadata.is_ok()
}

#[tokio::main]
async fn main() {

    println!("ZCash Params Downloader v{VERSION} (q) Decker, 2023");

    let zcash_params_dir_str = get_zcash_params_directory().expect("Unable to get Zcash params directory");

    let zcash_params_dir = PathBuf::from(&zcash_params_dir_str);

    tokio::fs::create_dir_all(&zcash_params_dir).await.unwrap_or_else(|why| {
        println!("! {:?}", why.kind());
    });

    println!("Zcash params directory: {}", zcash_params_dir.display());

    let filenames = [
        "sprout-proving.key.deprecated-sworn-elves",
        "sprout-verifying.key",
        "sapling-spend.params",
        "sapling-output.params",
        "sprout-groth16.params"
    ];

    let hashes = [
        "8bc20a7f013b2b58970cddd2e7ea028975c88ae7ceb9259a5344a16bc2c0eef7",
        "4bd498dae0aacfd8e98dc306338d017d9c08dd0918ead18172bd0aec2fc5df82",
        "8e48ffd23abb3a5fd9c5589204f32d9c31285a04b78096ba40a79b75677efc13",
        "2f0ebbcbb9bb0bcffe95a397e7eba89c29eb4dde6191c339db88570e3f3fb0e4",
        "b685d700c60328498fbde589c8c7c484c722b788b265b72af448a5bf0ee55b50"
    ];


    let https = hyper_rustls::HttpsConnectorBuilder::new()
        .with_native_roots()
        .https_or_http()
        .enable_http1()
        .enable_http2()
        .build();

    let client = Client::builder().build::<_, hyper::Body>(https);

    let base_url = "https://z.cash/downloads";
    let multi = Arc::new(MultiProgress::new());

    let mut tasks = Vec::new();

    for filename in filenames {
        let url = format!("{}{}", base_url, filename);
        let local_filename = match filename {
            "sprout-proving.key.deprecated-sworn-elves" => "sprout-proving.key",
            _ => filename
        };

        let mut full_path = PathBuf::from(&zcash_params_dir_str);
        full_path.push(local_filename);
        let full_path_str = full_path.to_str().expect("Path contains invalid Unicode characters").to_string();

        
        if !file_exists(&full_path_str).await {
            let client = client.clone();
            let multi = multi.clone();

            // Create new Task for each filename and add to the tasks Vec
            let task = tokio::spawn(async move {
                if let Err(e) = download_file(&client, url, full_path_str, multi).await {
                    println!("Error downloading {}: {:?}", local_filename, e);
                }
            });

            tasks.push(task);
        }
    }

    for task in tasks {
        if let Err(e) = task.await {
            println!("Task join error: {:?}", e);
        }
    }

    
    for (filename, hash) in filenames.iter().zip(hashes.iter()) {

        let local_filename = match *filename {
            "sprout-proving.key.deprecated-sworn-elves" => "sprout-proving.key",
            _ => filename
        };

        let mut full_path = PathBuf::from(&zcash_params_dir_str);
        full_path.push(local_filename);
        let full_path_str = full_path.to_str().expect("Path contains invalid Unicode characters").to_string();

        if file_exists(&full_path_str).await {
            print!("Filename: {}", full_path_str);
            let mut file = match File::open(&full_path_str).await {
                Ok(file) => file,
                Err(error) => {
                    println!("An error occurred while opening the file: {}", error);
                    continue; // Skip the rest of the loop
                }
            };

            match validate_checksum(&mut file, hash).await {
                Ok(_) => {
                    println!(" - Ok!")
                },
                Err(e) => {
                    println!(" - Error: {}", e);
                }
            };
            
        }
    }
}

async fn download_file(client: &Client<HttpsConnector<HttpConnector>>, url: String, filename:String, multi: Arc<MultiProgress>) -> Result<(), DownloadError> {
    // Try to open the file.
    let mut file = File::create(&filename).await?;

    let mut uri = url.parse::<Uri>().map_err(|_err| DownloadError::UrlParse)?;
    
    let mut response: Response<_>;

    loop {
        // Try to fetch the URL.
        let req = Request::builder()
            .method(Method::GET)
            .uri(uri)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/58.0.3029.110 Safari/537.3")
            .body(Body::from(""))
            .unwrap();

        response = client.request(req).await?;
        // response = client.get(uri.to_owned()).await?;

        // Check if the response is a redirect.
        if response.status().is_redirection() {
            // Get the redirect location.
            let location = response.headers().get("location").ok_or(DownloadError::LocationError)?;
            let location_str = location.to_str().map_err(|_err| DownloadError::LocationError)?;
            uri = location_str.parse::<hyper::Uri>().map_err(|_err| DownloadError::LocationError)?;
        } else {
            break;
        }
    }

    // Process the stream.
    // let stream = response.into_body();

    // We should not use this here because the files are too big. Instead, we should receive the response in chunks and write these chunks to a file while displaying the progress.
    // let data = hyper::body::to_bytes(stream).await?; 

    assert_eq!(response.status(), StatusCode::OK);
    // assert!(response.headers().get(hyper::http::header::CONTENT_DISPOSITION).is_none());

    let content_length = response.headers()
        .get(hyper::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse().ok())
        .unwrap_or(0);

    let progress_bar = multi.add(ProgressBar::new(content_length));
    // progress_bar.set_length(content_length);
    let template_begin_str = r#"{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})"#;
    let template = format!("{} | {}", template_begin_str, filename);

    // "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({eta})"
    progress_bar.set_style(ProgressStyle::default_bar()
        .template(&template).unwrap());
    // make sure we show up at all.  otherwise no rendering // event.
    progress_bar.tick();

    let mut body = response.into_body();
    
    let mut bytes_downloaded = 0_usize;

    while let Some(buf) = body.data().await {
        let buf = buf?;
        bytes_downloaded += buf.len();
        progress_bar.set_position(bytes_downloaded as u64);
        file.write_all(&buf).await?;
    }
    
    // progress_bar.finish();
    // progress_bar.finish_with_message("download complete");
    progress_bar.finish_and_clear();

    Ok(())

}

pub async fn validate_checksum(file: &mut File, checksum: &str) -> Result<(), ChecksumError> {

    let expected = <[u8; 32]>::from_hex(checksum)
        .map(GenericArray::from)
        .map_err(|_| ChecksumError::InvalidInput)?;

    let mut hasher = Sha256::new();
    let mut buffer = vec![0u8; 8 * 1024];

    loop {
        match file.read(&mut buffer).await? {
            0 => break,
            read => hasher.update(&buffer[..read]),
        }
    }

    let found = hasher.finalize();

    // let mut hash_string = String::new();
    // for byte in found.iter() {
    //     hash_string.push_str(&format!("{:02x}", byte));
    // }
    // println!("{}", hash_string);

    if *found != *expected {
        return Err(ChecksumError::Checksum {
            expected: checksum.into(),
            found:    format!("{:x}", found),
        });
    }

    Ok(())
}

fn get_zcash_params_directory() -> Result<String, Box<dyn std::error::Error>> {

    // Windows < Vista: C:\Documents and Settings\Username\Application Data\ZcashParams
    // Windows >= Vista: C:\Users\Username\AppData\Roaming\ZcashParams
    // Mac: ~/Library/Application Support/ZcashParams
    // Unix: ~/.zcash-params

    let mut zcash_params_directory: PathBuf;

    if cfg!(target_os = "windows") {
        zcash_params_directory = dirs::config_dir().ok_or("Config directory not found")?;
        zcash_params_directory.push("ZcashParams");
    } else if cfg!(target_os = "macos") {
        zcash_params_directory = dirs::home_dir().ok_or("Home directory not found")?;
        zcash_params_directory.push("Library/Application Support/ZcashParams");
    } else {
        zcash_params_directory = dirs::home_dir().ok_or("Home directory not found")?;
        zcash_params_directory.push(".zcash-params");
    }

    Ok(zcash_params_directory.to_str().unwrap().to_string())
}