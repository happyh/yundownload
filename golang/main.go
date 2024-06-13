package main

import (
	"bufio"
	"fmt"
	"io"
	"math"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"regexp"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/spf13/pflag"
)

type downloadInfo struct {
	Url     string
	Headers map[string]string
}
type Task struct {
	Index   int
	Percent int
	Speed   int
}

func main() {
	noScreen := pflag.Bool("noscreen", false, "是否关闭screen模式")
	pflag.Parse() // 解析命令行参数

	positionalArgs := pflag.Args()
	if len(positionalArgs) != 1 {
		fmt.Println("必须指定ef2格式文件的名称，%v", positionalArgs)
		os.Exit(1)
	}

	ef2File := positionalArgs[0]

	// 根据noScreen的值执行不同的逻辑
	if *noScreen {
		dowdownloadef2(ef2File)
	} else {
		pwd, _ := os.Getwd()
		fmt.Printf("当前目录：%s, 下载文件：%s 已经后台screen执行，可screen -r进入查看下载进度\n", pwd, ef2File)

		// 构造要执行的命令
		command := fmt.Sprintf("./%s %s --noscreen", os.Args[0], ef2File)

		// 使用screen执行命令
		cmd := exec.Command("screen", "-dmS", "my_screen", "bash", "-c", command)
		if err := cmd.Start(); err != nil {
			fmt.Println("执行screen命令时出错:", err)
			os.Exit(1)
		}
	}
}
func dowdownloadef2(ef2filename string) {
	infos, err := parseDownloadInfo(ef2filename)
	if err != nil {
		fmt.Println("File parsing error:", err)
		return
	}

	var wg sync.WaitGroup
	for i, info := range infos {
		wg.Add(1)
		go func(i int, info downloadInfo) {
			defer wg.Done()

			downloadResource(info)
		}(i, *info)

	}
	wg.Wait()
}

func parseDownloadInfo(filePath string) ([]*downloadInfo, error) {
	contentBytes, err := os.ReadFile(filePath)
	if err != nil {
		return nil, err
	}

	content := strings.ReplaceAll(string(contentBytes), "\r\n", "\n")

	regexPattern := `<\n(.*?)\nreferer: (.*?)\nUser-Agent: (.*?)\n>`
	compiledRegex := regexp.MustCompile(regexPattern)

	matches := compiledRegex.FindAllStringSubmatch(content, -1)

	infos := make([]*downloadInfo, 0, len(matches))

	for _, match := range matches {
		info := &downloadInfo{
			Url: strings.TrimSpace(match[1]),
			Headers: map[string]string{
				"referer":    strings.TrimSpace(match[2]),
				"User-Agent": strings.TrimSpace(match[3]),
			},
		}
		infos = append(infos, info)
	}

	return infos, nil
}

func downloadResource(info downloadInfo) {
	// 创建一个临时目录来保存分片
	dir := "downloading"
	err := os.Mkdir(dir, os.ModePerm)
	if err != nil && !os.IsExist(err) {
		fmt.Println("创建临时目录失败:", err)
		return
	}
	//defer os.RemoveAll(dir) // 最后清理临时文件

	filename, filesize, err := downloadHeader(info)
	if err != nil {
		fmt.Println("获取文件头信息失败:", err)
		return
	}

	var numParts = 10
	if filesize > 10*1024*1024*1024 {
		numParts = int(filesize/1024*1024*1024) + 1
	}
	if filesize < 1024 {
		numParts = 1
	}

	part_size := filesize / int64(numParts)

	task_info := make(chan Task)
	defer close(task_info)

	partfilename := make([]string, numParts)
	outfilename := dir + "/" + filename
	// 分别下载每个分片
	var wg sync.WaitGroup
	for i := 0; i < numParts; i++ {
		range_begin := int64(i) * part_size
		range_end := int64(i+1)*part_size - 1
		if range_end > filesize {
			range_end = filesize - 1
		}
		wg.Add(1)
		go func(index int, range_begin, range_end int64) {
			defer wg.Done()
			fullfileanme := outfilename + "." + strconv.FormatInt(range_begin, 10) + "-" + strconv.FormatInt(range_end, 10)
			partfilename[index] = fullfileanme
			err := downloadPart(index, info, range_begin, range_end, fullfileanme, task_info)
			if err != nil {
				fmt.Printf("下载分片%d失败: %v\n", index, err)
			} else {
				fmt.Printf("分片%d下载完成。\n", index)
			}
		}(i, range_begin, range_end)
	}

	tick := time.Tick(2 * time.Second)
	arTasks := make([]Task, numParts)
	for {
		finish := 0
		select {
		case p := <-task_info:
			arTasks[p.Index] = p
		case <-tick:
			var Percent = 0
			var Speed = 0
			for _, v := range arTasks {
				if v.Percent >= 100 {
					finish++
				}
				Percent += v.Percent
				Speed += v.Speed
			}
			leftTime := calculateLeftTime(int64(Speed), filesize, filesize*int64(Percent)/100)
			fmt.Printf("\r%s", filename+": "+strconv.Itoa(Percent/numParts)+"% "+humanSize(int64(Speed))+" "+leftTime)
		}
		if finish >= numParts {
			break
		} else {
		}
	}

	wg.Wait()

	mergedFiles(partfilename, outfilename)
}

func downloadHeader(info downloadInfo) (filename string, filesize int64, err error) {
	req, err := http.NewRequest("GET", info.Url, nil)
	if err != nil {
		return "", 0, err
	}
	req.Header.Set("Range", "bytes=0-0")
	for key, value := range info.Headers {
		req.Header.Set(key, (value))
	}

	client := &http.Client{}
	resp, err := client.Do(req)
	if err != nil {
		return "", 0, err
	}
	defer resp.Body.Close()

	// 检查响应状态码，确保是成功或部分内容（206）
	if resp.StatusCode != http.StatusOK && resp.StatusCode != http.StatusPartialContent {
		return "", 0, fmt.Errorf("请求文件名信息失败，状态码:%d,url:%s", resp.StatusCode, info.Url)
	}

	return parseFilenameFromDisposition(resp.Header.Get("Content-Disposition"), resp.Header.Get("Content-Range"))
}

func parseFilenameFromDisposition(disposition string, Content_Range string) (filename string, filesize int64, err error) {
	re := regexp.MustCompile(`filename\*=UTF-8\'\'(.+)`)
	match := re.FindStringSubmatch(disposition)
	if len(match) < 1 {
		return "", 0, fmt.Errorf("如果没有找到filename参数")
	}

	filename, err = url.QueryUnescape(match[1])
	if err != nil {
		return "", 0, fmt.Errorf("Error decoding filename: %v", err)
	}

	filename = sanitizeFilename(filename)

	match = strings.Split(Content_Range, "/") //bytes 0-0/1256
	if len(match) != 2 {
		return "", 0, fmt.Errorf("Content-Range格式错误:%v", Content_Range)
	}

	filesize, err = strconv.ParseInt(match[1], 10, 64)
	if err != nil {
		return "", 0, fmt.Errorf("解析文件大小错误 %v", match[2])
	}

	return filename, filesize, nil

}

func downloadPart(index int, info downloadInfo, range_begin int64, range_end int64, fullfilename string, p chan<- Task) error {
	req, err := http.NewRequest("GET", strings.TrimSpace(info.Url), nil)
	if err != nil {
		return err
	}

	file, err := os.OpenFile(fullfilename, os.O_WRONLY|os.O_CREATE|os.O_APPEND, 0644)
	if err != nil {
		return err
	}
	defer file.Close()

	fi, err := file.Stat()
	if err != nil {
		return err
	}
	filesize := fi.Size()

	req.Header.Set("Range", fmt.Sprintf("bytes=%d-%d", filesize+range_begin, range_end))
	for key, value := range info.Headers {
		req.Header.Set(key, strings.TrimSpace(value))
	}

	client := &http.Client{}
	resp, err := client.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	// 检查响应状态码，确保是成功或部分内容（206）
	if resp.StatusCode != http.StatusOK && resp.StatusCode != http.StatusPartialContent {
		return fmt.Errorf("请求分片失败，状态码：%d", resp.StatusCode)
	}
	raw := resp.Body

	buffer_size := 10 * 1024
	buff := make([]byte, buffer_size)
	file.Seek(0, io.SeekEnd)
	reader := bufio.NewReaderSize(raw, buffer_size)
	writer := bufio.NewWriter(file)
	lasttime := time.Now().Unix()
	time_interval := 1
	last_receive := 0
	var task Task
	task.Index = index
	written := 0
	for {
		nr, er := reader.Read(buff)
		if nr > 0 {
			nw, ew := writer.Write(buff[0:nr])
			if nw > 0 {
				written += nw
				last_receive += nw
				current_time := time.Now().Unix()
				if int(current_time-lasttime) > time_interval {
					task.Percent = (written + int(filesize)) * 100 / int(resp.ContentLength+filesize)
					task.Speed = last_receive / int(current_time-lasttime)
					p <- task
					lasttime = time.Now().Unix()
					last_receive = 0
				}

			}
			if ew != nil {
				err = ew
				break
			}
			if nr != nw {
				err = io.ErrShortWrite
				break
			}
		}
		if er != nil {
			if er != io.EOF {
				err = er
			}
			break
		}
	}
	raw.Close()
	writer.Flush()

	return err
}

func calculateLeftTime(speed int64, contentSize int64, downloadedSize int64) string {
	if speed == 0 {
		return "NaN"
	}

	remainingBytes := contentSize - downloadedSize
	remainingSeconds := int64(float64(remainingBytes) / float64(speed))
	duration := time.Duration(remainingSeconds) * time.Second
	return duration.String()
}

func mergedFiles(files []string, outputFile string) error {
	outFile, err := os.Create(outputFile)
	if err != nil {
		return fmt.Errorf("Error creating output file:%v", err)
	}
	defer outFile.Close()

	// 逐个打开文件并合并内容
	for _, filename := range files {
		// 打开文件
		inFile, err := os.Open(filename)
		if err != nil {
			fmt.Printf("Error opening file %s: %v\n", filename, err)
			continue
		}
		defer inFile.Close()

		// 复制文件内容到输出文件
		_, err = io.Copy(outFile, inFile)
		if err != nil {
			return fmt.Errorf("Error writing to output file:%v", err)
		}
	}

	return nil
}

func humanSize(sizeBytes int64) string {
	if sizeBytes == 0 {
		return "0B"
	}

	const unit = 1024
	units := []string{"B", "KB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB"}
	i := int(math.Floor(math.Log(float64(sizeBytes)) / math.Log(float64(unit))))
	p := int64(math.Pow(float64(unit), float64(i)))
	s := float64(sizeBytes) / float64(p)
	// 格式化到小数点后两位
	s = math.Round(s*100) / 100

	return fmt.Sprintf("%.2f %s", s, units[i])
}

func sanitizeFilename(filename string) string {
	// 非法字符列表
	illegalChars := "/\\:*?\"<>|"

	// 遍历非法字符列表，并使用strings.ReplaceAll替换它们
	for _, char := range illegalChars {
		filename = strings.ReplaceAll(filename, string(char), "_")
	}

	return filename
}
