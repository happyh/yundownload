use std::fs::File;
use std::io::{BufRead, BufReader};
use std::process::Command;
use std::time::Duration;
fn main() -> std::io::Result<()> {
    let filename = "file.txt";
    let file = File::open(filename)?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let command = format!("screen -dmS my_screen bash -c '{}'", line?);
        let output = Command::new("sh")
            .arg("-c")
            .arg(command)
            .output()?;
        println!("{}", String::from_utf8_lossy(&output.stdout));
        println!("{}", String::from_utf8_lossy(&output.stderr));
        std::thread::sleep(Duration::from_millis(100)); // 等待100毫秒，避免同时启动过多的screen会话
    }
    Ok(())
}
