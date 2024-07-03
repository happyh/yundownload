use std::fs::File;
use std::io::{BufRead, BufReader, Error};
use std::collections::HashMap;
use regex::Regex;

// 类似于Go的downloadInfo结构体
#[derive(Debug)]
struct DownloadInfo {
    url: String,
    headers: HashMap<String, String>,
}

impl DownloadInfo {
    fn new(url: String, headers: HashMap<String, String>) -> Self {
        DownloadInfo { url, headers }
    }
}

fn parse_download_info(file_path: &str) -> Result<Vec<DownloadInfo>, Error> {
    let lower_file_path = file_path.to_lowercase();
    if lower_file_path.starts_with("http://") || lower_file_path.starts_with("https://") {
        let headers = HashMap::from([
            ("User-Agent".to_string(), "Mozilla/5.0 (Windows NT 10.0; WOW64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/53.0.2763.0 Safari/537.36".to_string()),
        ]);
        return Ok(vec![DownloadInfo::new(file_path.to_string(), headers)]);
    }

    let file = File::open(file_path)?;
    let reader = BufReader::new(file);
    let content = reader.lines().fold(String::new(), |acc, line| acc + &line.unwrap() + "\n");

    let regex_pattern = r"<\r?\n(.*?)\r?\nreferer: (.*?)\r?\nuser-agent: (.*?)\r?\n>";
    let regex = Regex::new(regex_pattern).unwrap();

    let captures = regex.captures_iter(&content);

    let infos = captures.map(|cap| {
        let url = cap.get(1).unwrap().as_str().trim();
        let referer = cap.get(2).unwrap().as_str().trim();
        let user_agent = cap.get(3).unwrap().as_str().trim();
        let headers = HashMap::from([
            ("referer".to_string(), referer.to_string()),
            ("User-Agent".to_string(), user_agent.to_string()),
        ]);
        DownloadInfo::new(url.to_string(), headers)
    }).collect();

    Ok(infos)
}

fn main() {
    let result = parse_download_info("path/to/your/file.txt");
    match result {
        Ok(infos) => {
            for info in infos {
                println!("{:?}", info);
            }
        },
        Err(e) => {
            eprintln!("Error: {}", e);
        }
    }
}