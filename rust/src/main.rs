use std::fs::{self,File, OpenOptions};
use std::io::{self,BufReader, BufWriter, Read,BufRead, Write, };
use regex::Regex;
use clap::{App, Arg};
use std::env;
use std::process::Command;
use std::convert::TryInto;
use reqwest::header::{HeaderMap,HeaderValue,HeaderName,InvalidHeaderValue};
use reqwest::blocking::{Client, ClientBuilder, Response};

use std::sync::mpsc::{channel, Sender, Receiver};
use std::sync::mpsc;

use std::sync::{Arc, Barrier,Mutex,RwLock};
use std::thread;
use std::time::{Instant,Duration, SystemTime, UNIX_EPOCH};
use std::error::Error;
use std::fmt;


#[derive(Debug)]
#[derive(Clone)]
struct DownloadInfo {
    Url: String,
    Headers: HeaderMap,
}

impl DownloadInfo {
    fn new(Url: String, Headers: HeaderMap) -> Self {
        DownloadInfo { Url, Headers }
    }
}


fn download_ef2(ef2_filename: &str, parallel: usize) {
    let infos = match parse_download_info(ef2_filename) {
        Ok(infos) => infos,
        Err(e) => {
            println!("File parsing error: {}", e);
            return;
        }
    };

    let barrier = Arc::new(Barrier::new(infos.len()));
    let infos_len = infos.len();
    for (_i, info) in infos.into_iter().enumerate() {
        let barrier = Arc::clone(&barrier);

        thread::spawn(move || {
            match download_resource(&info, parallel, infos_len) {
                Ok(()) => (),
                Err(e) => println!("Download resource error: {}", e),
            }

            barrier.wait();
        });

        thread::sleep(Duration::from_secs(2));
    }

    barrier.wait();
}

#[derive(Debug)]
enum CustomError {
    Io(io::Error),
    InvalidHeaderValue(InvalidHeaderValue),
    Other(Box<dyn OtherError>),
    Other2(Box<dyn Error + Send + Sync + 'static>),
}

impl fmt::Display for CustomError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CustomError::Io(err) => write!(f, "I/O error: {}", err),
            CustomError::InvalidHeaderValue(err) => write!(f, "Invalid header value: {}", err),
            CustomError::Other(e) => write!(f, "Other error: {}", e),
            CustomError::Other2(e) => write!(f, "Other error: {}", e),
        }
    }
}

impl From<io::Error> for CustomError {
    fn from(err: io::Error) -> Self {
        CustomError::Io(err)
    }
}

impl From<InvalidHeaderValue> for CustomError {
    fn from(err: InvalidHeaderValue) -> Self {
        CustomError::InvalidHeaderValue(err)
    }
}
impl From<Box<dyn OtherError>> for CustomError {
    fn from(e: Box<dyn OtherError>) -> Self {
        CustomError::Other(e)
    }
}

impl From<Box<dyn Error + Send + Sync + 'static>> for CustomError {
    fn from(e: Box<dyn Error + Send + Sync + 'static>) -> Self {
        CustomError::Other2(e)
    }
}

fn parse_download_info(file_path: &str) -> Result<Vec<DownloadInfo>, CustomError> {
    let lower_file_path = file_path.to_lowercase();
    if lower_file_path.starts_with("http://") || lower_file_path.starts_with("https://") {
        let mut header_map = HeaderMap::new();
        let value = "Mozilla/5.0 (Windows NT 10.0; WOW64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/53.0.2763.0 Safari/537.36";
        let header_value = HeaderValue::from_str(value)?;

        header_map.insert("User-Agent", header_value);

        return Ok(vec![DownloadInfo::new(file_path.to_string(), header_map)]);
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

        let referer_header_name = HeaderName::from_static("referer");
        let user_agent_header_name = HeaderName::from_static("User-Agent");

        let referer_header_value = HeaderValue::from_str(referer).map_err(|e| CustomError::from(e))?;
        let user_agent_header_value = HeaderValue::from_str(user_agent).map_err(|e| CustomError::from(e))?;

        let mut header_map = HeaderMap::new();
        header_map.insert(referer_header_name, referer_header_value);
        header_map.insert(user_agent_header_name, user_agent_header_value);

        Ok::<_, CustomError>(DownloadInfo::new(url.to_string(), header_map))
    }).collect::<Result<Vec<_>, CustomError>>()?;

    Ok(infos)
}


fn main() {
    let matches = App::new("ef2_downloader")
        .version("1.0")
        .author("Your Name <your.email@example.com>")
        .about("Downloads EF2 files with optional screen support")
        .arg(
            Arg::with_name("noscreen")
                .short('s')
                .long("noscreen")
                .help("Disable screen mode")
                .takes_value(false)
        )
        .arg(
            Arg::with_name("parallel")
                .short('p')
                .long("parallel")
                .help("Number of parallel downloads")
                .default_value("10")
        )
        .arg(
            Arg::with_name("ef2_files")
                .help("EF2 files to download")
                .required(true)
                .multiple(true)
        )
        .get_matches();

    let no_screen = matches.is_present("noscreen");
    let parallel = matches.value_of("parallel").unwrap().parse::<u32>().unwrap();
    let ef2_files = matches.values_of_os("ef2_files").unwrap().collect::<Vec<_>>();

    if ef2_files.is_empty() {
        eprintln!("Must specify EF2 file names");
        std::process::exit(1);
    }

    for ef2_file in ef2_files {
        let pwd = env::current_dir().unwrap().display().to_string();
        let ef2_filename = ef2_file.to_str().unwrap_or_else(|| panic!("Path is not valid UTF-8"));
        if no_screen {
            println!("Current directory: {:?}, download file: {:?}", pwd, ef2_filename);
            download_ef2(&ef2_filename, parallel.try_into().unwrap());
        } else {
            println!("Current directory: {:?}, download file: {} has been executed in background screen, you can use 'screen -r' to check the download progress.", pwd, ef2_filename);
            let args: Vec<String> = env::args().collect();
            let command = format!("{} {} --noscreen -p {}", args[0], ef2_filename, parallel);
            match Command::new("screen")
                .args(&["-dmS", "my_screen", "bash", "-c", &command])
                .output() {
                Ok(output) => {
                    println!("Standard output: {}", String::from_utf8_lossy(&output.stdout));
                    println!("Standard error: {}", String::from_utf8_lossy(&output.stderr));
                },
                Err(e) => {
                    println!("An error occurred: {}", e);
                },
            }
        }
    }
}

fn download_resource(info: &DownloadInfo,mut parallel: usize, all_task_count: usize) -> Result<(),CustomError> {
    let (filename, filesize, crc64) = download_header(info)?;
    let filesize = filesize as u64;
    let mut file_info = fs::metadata(&filename).ok();
    if let Some(fi) = file_info.as_ref() {
        if fi.len() == filesize {
            println!("文件已经存在: {}", filename);
            return Ok(());
        } else if fi.len() != filesize {
            println!("文件已经存在: {}, 但是文件大小不对，重新下载", filename);
        }
    }

    let pwd = env::current_dir().unwrap();
    let lock_file_name = pwd.join(format!("{}.lock", filename));
    let lock_file = Arc::new(RwLock::new(None::<std::fs::File>));

    {
        let mut lock = lock_file.write().unwrap();
        *lock = Some(std::fs::OpenOptions::new().create(true).open(&lock_file_name)?);
    }

    let temp_dir = pwd.join(format!("{}downloading", filename));
    fs::create_dir_all(&temp_dir)?;

    let filesize = filesize as usize;
    if filesize > 10 * 1024 * 1024 * 1024 && parallel == 10 {
        parallel = (filesize / (1024 * 1024 * 1024)) + 1;
    }
    if filesize < 1024 * 1024 && parallel == 10 {
        parallel = (filesize / (100 * 1024)) + 1;
    }

    let part_size = ((filesize - 1) / parallel) + 1;

    let (tx, rx) = channel();
    let mut part_filenames = vec![String::new(); parallel as usize];
    let mut handles = vec![];
    for i in 0..parallel {
        let range_begin = i * part_size;
        let range_end = if i + 1 == parallel {
            filesize - 1
        } else {
            (i + 1) * part_size - 1
        };
        let full_filename = temp_dir.join(format!("{}.{}-{}", filename, range_begin, range_end)).to_str().unwrap().to_string();
        part_filenames[i as usize] = full_filename.clone();

        let info = info.clone();
        let tx = tx.clone();
        let lock_file = lock_file.clone();
        let handle = thread::spawn(move || {
            let result = download_part(i, info, range_begin.try_into().unwrap(), range_end.try_into().unwrap(), &full_filename, tx);
        });
        handles.push(handle);
    }

    drop(tx);
    let mut ar_tasks = vec![Task { index: 0, percent: 0, speed: 0, errstring: "".to_string() }; parallel];
    let mut tick = Instant::now();
    let mut is_error = false;
    let mut finish = 0;
    while let Ok(task) = rx.recv() {
        if !task.errstring.is_empty(){
            is_error = true;
        }
        if task.percent >= 100 {
            finish += 1;
        }
        if finish >= parallel as isize {
            println!("finish");
            break;
        }
        if tick.elapsed().as_secs() > all_task_count as u64 * 2 {
            tick = Instant::now();
            let mut percent_sum = 0;
            let mut speed_sum = 0;
            for task in &ar_tasks {
                percent_sum += task.percent;
                speed_sum += task.speed;
            }
            let percent_avg = percent_sum as f64 / parallel as f64;
            let speed_avg = speed_sum as f64 / parallel as f64;
            let left_time = calculate_left_time(speed_avg as i64, filesize.try_into().unwrap(), (percent_avg * filesize as f64 / 100.0) as i64);
            println!("\r{}: {:.2}% {} {}", filename, percent_avg, human_size(speed_avg as i64), left_time);
        }
    }

    for handle in handles {
        handle.join().unwrap();
    }

    if !is_error {
        let output_file_str = pwd.join(filename).to_str().unwrap().to_string();
        let result = merged_files(part_filenames, output_file_str.to_string(), crc64);
        if result.is_ok() {
            fs::remove_dir_all(temp_dir)?;
        } else {
            println!("合并文件失败，err: {:?}", result.err());
        }
    }

    Ok(())
}

use reqwest::Url;
use std::error::Error as OtherError;
use percent_encoding::{percent_decode_str};

fn download_header(info: &DownloadInfo) -> Result<(String, i64, u64), Box<dyn OtherError>> {
    let headers = info.Headers.iter().fold(HeaderMap::new(), |mut acc, (k, v)| {
        acc.append(k.clone(), v.clone());
        acc
    });
    let client = Client::new();
    let response = client
        .get(&info.Url)
        .header("Range", HeaderValue::from_static("bytes=0-0"))
        .headers(headers)
        .send()?;

    if !response.status().is_success() && response.status() != reqwest::StatusCode::PARTIAL_CONTENT {
        return Err(format!("请求文件名文件大小等信息失败，状态码:{},url:{}", response.status(), info.Url).into());
    }

    let content_disposition = response.headers().get("content-disposition").and_then(|h| h.to_str().ok());

    let re = Regex::new(r"filename\*=UTF-8''(.+)")?;
    let filename = if let Some(content_disposition) = content_disposition {
        if let Some(caps) = re.captures(content_disposition) {
            if let Some(m) = caps.get(1) {
                let matched = m.as_str();
                percent_decode_str(matched)
                    .decode_utf8_lossy()
                    .into_owned()
            } else {
                "".into() // 返回空字符串的解码结果
            }
        } else {
            let parsed_url = Url::parse(&info.Url)?;
            let path_segment = parsed_url.path_segments().and_then(|segments| segments.last());
            percent_decode_str(path_segment.map_or("", |s| s))
                .decode_utf8_lossy()
                .into_owned()
        }
    } else {
        let parsed_url = Url::parse(&info.Url)?;
        let path_segment = parsed_url.path_segments().and_then(|segments| segments.last());
        percent_decode_str(path_segment.map_or("", |s| s))
            .decode_utf8_lossy()
            .into_owned()
    };


    let filename = sanitize_filename(filename);

    let content_range = response.headers().get("content-range").and_then(|h| h.to_str().ok());
    let (start, end, total) = if let Some(content_range) = content_range {
        let parts: Vec<&str> = content_range.split('/').collect();
        if parts.len() != 2 {
            return Err("Content-Range格式错误".into());
        }
        let total = parts[1].parse::<i64>()?;
        (parts[0].split('-').collect::<Vec<&str>>()[0].parse::<i64>()?, parts[0].split('-').collect::<Vec<&str>>()[1].parse::<i64>()?, total)
    } else {
        return Err("Content-Range header not found".into());
    };

    let filesize = total;

    let crc64 = response
        .headers()
        .get("x-oss-hash-crc64ecma")
        .and_then(|h| h.to_str().ok())
        .map(|s| s.trim())
        .and_then(|s| s.parse::<u64>().ok())
        .ok_or("无法解析CRC64")?;

    Ok((filename, filesize, crc64))
}

/// 清洗文件名
fn sanitize_filename(filename: String) -> String {
    let illegal_chars = r#"\/:*?"<>|"#;
    illegal_chars.chars().fold(filename, |acc, c| acc.replace(c, "_"))
}




// 定义 Task 结构
#[derive(Debug, Clone)]
struct Task {
    index: usize,
    errstring: String,
    speed: i64,
    percent: i32,
}

fn download_part(
    index: usize,
    info: DownloadInfo,
    range_begin: i64,
    range_end: i64,
    fullfilename: &str,
    p: Sender<Task>,
) -> Result<(), Box<dyn std::error::Error>> {
    let client = Client::new();
    let req = client.get(info.Url.trim())
        .header("Range", format!("bytes={}-{}", range_begin, range_end))
        .headers(info.Headers)
        .build()?;

    let filesize = match std::fs::metadata(fullfilename) {
        Ok(metadata) => metadata.len(),
        Err(_) => 0,
    };

    let mut task = Task {
        index,
        errstring: "".to_string(),
        speed: 0,
        percent: 0,
    };

    if filesize > (range_end - range_begin) as u64 {
        println!("{} 已经下载完成, filesize: {}, rangesize: {}", index, filesize, range_end - range_begin + 1);
        task.percent = 100;
        p.send(task).unwrap();
        return Ok(());
    } else {
        println!("{} 继续下载, filesize: {}, rangesize: {}", index, filesize, range_end - range_begin + 1);
    }

    // 发送请求
    let mut resp = client.execute(req)?;
    if resp.status().is_success() || resp.status().as_u16() == 206 {
        let content_length = resp.content_length().unwrap();
        let mut file = OpenOptions::new().create(true).append(true).open(fullfilename)?;
        let mut buffer = vec![0; 10 * 1024];
        let mut writer = BufWriter::new(&mut file);
        let mut reader = BufReader::new(&mut resp);
        let mut written = 0;
        let mut last_receive = 0;
        let mut last_time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;

        loop {
            let nr = reader.read(&mut buffer)?;
            if nr > 0 {
                writer.write_all(&buffer[..nr])?;
                written += nr;
                last_receive += nr;
                let current_time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
                if current_time - last_time > 1 {
                    let new_task = Task {
                        index,
                        errstring: "".to_string(),
                        percent: ((written as u64 + filesize) * 100 / (content_length + filesize)) as i32,
                        speed: last_receive as i64 / (current_time - last_time),
                    };

                    p.send(new_task).unwrap();
                    last_time = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
                    last_receive = 0;
                }
            }
            if nr == 0 { break; }
        }
        let new_task = Task {
            index,
            errstring: "".to_string(),
            percent : 100,
            speed : 0
        };
        p.send(new_task).unwrap();
    } else {
        return Err(Box::new(io::Error::new(io::ErrorKind::Other, format!("请求分片失败，状态码：{}", resp.status()))));
    }

    Ok(())
}

/// 计算剩余时间
fn calculate_left_time(speed: i64, content_size: i64, downloaded_size: i64) -> String {
    if speed == 0 {
        return "NaN".to_string();
    }

    let remaining_bytes = content_size - downloaded_size;
    let remaining_seconds = (remaining_bytes as f64) / (speed as f64);
    let duration = Duration::from_secs(remaining_seconds as u64);

    format!("{:?}", duration)
}

/// 合并文件
fn merged_files(files: Vec<String>, output_file: String, crc64: u64) -> io::Result<()> {
    let mut out_file = File::create(&output_file)?;
    for filename in &files {
        let mut in_file = File::open(filename)?;
        io::copy(&mut in_file, &mut out_file)?;
    }

    if crc64 != 0 {
        let crc64check = crc64_checksum(&output_file)?;
        if crc64check != crc64 {
            return Err(io::Error::new(io::ErrorKind::Other,
                                      format!("crc64Checksum failed, crc64check: {}, crc64: {}", crc64check, crc64)));
        }
    }

    Ok(())
}

/// 格式化文件大小
fn human_size(size_bytes: i64) -> String {
    if size_bytes == 0 {
        return "0B".to_string();
    }

    let units = ["B", "KB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"];
    let unit: i32 = 1024;
    let i = ((size_bytes as f64).log(unit as f64)).floor() as usize;
    let p = unit.pow(i as u32);
    let s = (size_bytes as f64) / (p as f64);
    format!("{:.2}{}", s.round() / 100.0, units[i])
}

use crc64fast::Digest;
fn crc64_checksum(file_path: &str) -> Result<u64, io::Error> {
    // 打开文件
    let mut file = File::open(file_path)?;

    // 创建一个 CRC64 哈希计算器，并使用 ECMA 多项式初始化表
    let mut hasher = Digest::new();

    // 创建一个缓冲区用于读取文件内容
    let mut buffer = [0; 4096];
    loop {
        // 从文件中读取数据到缓冲区
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        // 更新 CRC64 哈希值
        hasher.write(&buffer[..bytes_read]);
    }

    // 获取最终的 CRC64 校验和
    Ok(hasher.sum64())
}