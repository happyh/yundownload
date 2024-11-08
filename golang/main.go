package main

import (
	"bufio"
	"fmt"
	"hash/crc64"
	"io"
	"math"
	"net/http"
	"net/url"
	"os"
	"os/exec"
	"path/filepath"
	"regexp"
	"runtime"
	"strconv"
	"strings"
	"sync"
	"time"

	log "github.com/happyh/go-logging"
	"github.com/happyh/pflag"
	"github.com/nightlyone/lockfile"
)

type downloadInfo struct {
	Url     string
	Headers map[string]string
}
type Task struct {
	Index   int
	Percent int
	Speed   int
	Err     error
}

var progressTick = 2 //每个任务2秒刷新一次显示进度

func main() {
	//使用cobra进行解析？
	var parallel int
	var cookie string
	var outputfilename string
	var referer string
	var taskfile string
	var noScreen bool
	pflag.IntVarP(&parallel, "parallel", "p", 10, "并发的协程数")
	pflag.StringVarP(&cookie, "cookie", "c", "", "cookie")
	pflag.StringVarP(&outputfilename, "out", "o", "", "保存文件名")
	pflag.StringVarP(&referer, "referer", "r", "", "referer 地址")
	pflag.BoolVarP(&noScreen, "noscreen", "s", false, "是否关闭screen模式，默认是screen模式")
	pflag.StringVarP(&taskfile, "taskfile", "t", "", "下载参数文件，如果指定则将文件的第一行读取解析为参数，目的是为了避免在命令行显示下载的url")

	pflag.Parse()

	switch runtime.GOOS {
	case "windows":
		noScreen = true
	}

	if noScreen && taskfile != "" {
		commandcotent, _ := readFirstLine(taskfile)
		commands := strings.Split(commandcotent, " ")
		pflag.ParseContent(commands)
		noScreen = true
	}

	logfilename := os.Args[0] + ".log"
	log.Init(logfilename, 6)

	positionalArgs := pflag.Args()
	if noScreen {
		var wg sync.WaitGroup
		progressTick = 2 * len(positionalArgs)
		for _, url := range positionalArgs {
			wg.Add(1)
			log.Log().Infof("下载url：%s \n", url)
			go func(url string) {
				defer wg.Done()
				dowdownloadTask(url, parallel, cookie, referer, outputfilename)
			}(url)
			time.Sleep(2 * time.Second)
		}
		wg.Wait()
	} else {
		// 构造要执行的命令
		command := fmt.Sprintf("%s --noscreen -p %d", os.Args[0], parallel)
		if referer != "" {
			command = command + " -r '" + referer + "'"
		}
		if cookie != "" {
			command = command + " -c '" + cookie + "'"
		}
		if outputfilename != "" {
			command = command + " -o '" + outputfilename + "'"
		}
		if taskfile != "" {
			command = command + " -t '" + taskfile + "'"
		}

		for _, url := range positionalArgs {
			command = command + " '" + url + "'"
		}
		pwd, _ := os.Getwd()
		log.Log().Infof("当前目录：%s, screen 后台执行任务%s\n", pwd, command)

		// 使用screen执行命令
		cmd := exec.Command("screen", "-dmS", "my_screen", "bash", "-c", command)
		if err := cmd.Start(); err != nil {
			log.Log().Error("执行screen命令时出错:", err)
			os.Exit(1)
		} else {
			log.Log().Info("执行screen命令是:", cmd)
		}
	}
}
func readFirstLine(filename string) (string, error) {
	file, err := os.Open(filename)
	if err != nil {
		log.Log().Infof("打开文件：%s 失败: %v", filename, err)
		return "", err
	}
	defer file.Close()

	reader := bufio.NewReader(file)
	line, err := reader.ReadString('\n')
	if err != nil && err != io.EOF {
		log.Log().Infof("读取文件：%s 失败: %v", filename, err)
		return "", err
	}

	// 去除行尾的换行符和空白字符
	line = strings.TrimSpace(line)
	if line == "" {
		log.Log().Infof("文件：%s 为空", filename)
		return "", fmt.Errorf("file is empty")
	}

	return line, nil
}

func dowdownloadTask(url string, parallel int, cookie, referer, outputfilename string) {
	// Check if the filePath is a URL.
	lowerurl := strings.ToLower(url)
	if strings.HasPrefix(lowerurl, "http://") || strings.HasPrefix(lowerurl, "https://") {
		info := downloadInfo{
			Url:     strings.TrimSpace(url),
			Headers: map[string]string{"User-Agent": "Mozilla/5.0 (Windows NT 10.0; WOW64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/53.0.2763.0 Safari/537.36"},
		}

		if cookie != "" {
			info.Headers["Cookie"] = cookie
		}
		if referer != "" {
			info.Headers["Referer"] = referer
		}

		downloadResource(info, parallel, outputfilename)
	}
}

func downloadResource(info downloadInfo, parallel int, outfilename string) {
	filename, filesize, crc64, err := downloadHeader(info)
	if err != nil {
		log.Log().Error("获取文件头信息失败:", err)
		return
	}

	if outfilename != "" {
		filename = outfilename
	}

	fileInfo, err := os.Stat(filename)
	if err == nil && filesize == fileInfo.Size() {
		log.Log().Info("文件已经存在:", filename)
		return
	} else if err == nil && filesize != fileInfo.Size() {
		log.Log().Info("文件已经存在:", filename, ",但是文件大小不对，重新下载")
	}

	pwd, _ := os.Getwd()
	// 创建一个新的锁文件实例
	lf, err := lockfile.New(pwd + "/" + filename + ".lock")
	if err != nil {
		log.Log().Errorf("创建锁文件时出错: %v", err)
		return
	}

	err = lf.TryLock()
	if err = lf.TryLock(); err != nil {
		log.Log().Errorf("Cannot lock %q, reason: %v", lf, err)
		return
	}
	defer lf.Unlock()

	// 创建一个临时目录来保存分片
	tmpdir := pwd + "/" + filename + "downloading"
	err = os.Mkdir(tmpdir, os.ModePerm)
	if err != nil && !os.IsExist(err) {
		log.Log().Error("创建临时目录失败:", err)
		return
	}

	if filesize > 10*1024*1024*1024 && parallel == 10 {
		parallel = int(filesize/(1024*1024*1024)) + 1
	}
	if filesize < 1024*1024 && parallel == 10 {
		parallel = int(filesize/(100*1024)) + 1
	}

	part_size := (filesize-1)/int64(parallel) + 1

	task_info := make(chan Task)
	defer close(task_info)

	partfilename := make([]string, parallel)
	var wg sync.WaitGroup
	for i := 0; i < parallel; i++ {
		range_begin := int64(i) * part_size
		range_end := int64(i+1)*part_size - 1
		if range_end >= filesize {
			range_end = filesize - 1
		}
		wg.Add(1)
		go func(index int, range_begin, range_end int64) {
			defer wg.Done()
			fullfileanme := tmpdir + "/" + filename + "." + strconv.FormatInt(range_begin, 10) + "-" + strconv.FormatInt(range_end, 10)
			partfilename[index] = fullfileanme
			err := downloadPart(index, info, range_begin, range_end, fullfileanme, task_info)
			if err != nil {
				log.Log().Errorf("%s 下载分片%d失败: %v\n", filename, index, err)
			} else {
				log.Log().Infof("%s 分片%d下载完成。\n", filename, index)
			}
		}(i, range_begin, range_end)
	}

	tick := time.Tick(time.Duration(progressTick) * time.Second)
	arTasks := make([]Task, parallel)
	isError := false
	for {
		finish := 0
		select {
		case p := <-task_info:
			arTasks[p.Index] = p
		case <-tick:
			var Percent = 0.0
			var Speed = 0
			for _, v := range arTasks {
				if v.Percent >= 100 {
					finish++
				}
				if v.Err != nil {
					isError = true
				}
				Percent += float64(v.Percent)
				Speed += v.Speed
			}
			Percent = Percent / float64(parallel)
			leftTime := calculateLeftTime(int64(Speed), filesize, int64(float64(filesize)*Percent/100))
			fmt.Printf("\r%s        ", filename+": "+strconv.FormatFloat(Percent, 'f', 2, 64)+"% "+humanSize(int64(Speed))+" "+leftTime)
		}
		if finish >= parallel {
			break
		} else {
		}
	}

	wg.Wait()

	if !isError {
		err = mergedFiles(partfilename, pwd+"/"+filename, crc64)
		if err == nil {
			os.RemoveAll(tmpdir)
		} else {
			log.Log().Error("合并文件失败，err:", err)
		}
	}
}

func downloadHeader(info downloadInfo) (filename string, filesize int64, crc64 uint64, err error) {
	req, err := http.NewRequest("GET", info.Url, nil)
	if err != nil {
		return "", 0, 0, err
	}
	req.Header.Set("Range", "bytes=0-0")
	for key, value := range info.Headers {
		req.Header.Set(key, (value))
	}

	client := &http.Client{}
	resp, err := client.Do(req)
	if err != nil {
		return "", 0, 0, err
	}
	defer resp.Body.Close()

	// 检查响应状态码，确保是成功或部分内容（206）
	if resp.StatusCode != http.StatusOK && resp.StatusCode != http.StatusPartialContent {
		return "", 0, 0, fmt.Errorf("请求文件名文件大小等信息失败，状态码:%d,url:%s", resp.StatusCode, info.Url)
	}

	re := regexp.MustCompile(`filename\*=UTF-8\'\'(.+)`)
	match := re.FindStringSubmatch(resp.Header.Get("Content-Disposition"))
	if len(match) < 1 {
		parsedURL, err := url.Parse(info.Url)
		if err != nil {
			return "", 0, 0, fmt.Errorf("获取url文件名失败")
		}
		filename, err = url.QueryUnescape(filepath.Base(parsedURL.Path))
		if err != nil {
			return "", 0, 0, fmt.Errorf("Error decoding filename: %v", err)
		}
	} else {
		filename, err = url.QueryUnescape(match[1])
		if err != nil {
			return "", 0, 0, fmt.Errorf("Error decoding filename: %v", err)
		}
	}
	filename = sanitizeFilename(filename)

	match = strings.Split(resp.Header.Get("Content-Range"), "/") //bytes 0-0/1256
	if len(match) != 2 {
		return "", 0, 0, fmt.Errorf("Content-Range格式错误:%v", resp.Header.Get("Content-Range"))
	}

	filesize, err = strconv.ParseInt(match[1], 10, 64)
	if err != nil {
		return "", 0, 0, fmt.Errorf("解析文件大小错误 %v", match[2])
	}

	crc64, err = strconv.ParseUint(strings.TrimSpace(resp.Header.Get("x-oss-hash-crc64ecma")), 10, 64)

	return filename, filesize, crc64, nil
}

func downloadPart(index int, info downloadInfo, range_begin int64, range_end int64, fullfilename string, p chan<- Task) error {
	req, err := http.NewRequest("GET", strings.TrimSpace(info.Url), nil)
	if err != nil {
		return err
	}
	filesize := int64(0)
	fileInfo, err := os.Stat(fullfilename)
	if err == nil {
		filesize = fileInfo.Size()
	}

	var task Task
	task.Index = index
	task.Err = nil
	task.Speed = 0
	if filesize > (range_end - range_begin) {
		log.Log().Info(index, " 已经下载完成,filesize:", filesize, ",rangesize:", range_end-range_begin+1)
		task.Percent = 100
		p <- task
		return nil
	} else {
		log.Log().Info(index, " 继续下载,filesize:", filesize, ",rangesize:", range_end-range_begin+1)
	}

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

	file, err := os.OpenFile(fullfilename, os.O_WRONLY|os.O_CREATE|os.O_APPEND, 0644)
	if err != nil {

		return err
	}
	defer file.Close()
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
	time_interval := 10
	last_receive := 0
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
					//fmt.Println(fullfilename, " speed ", task.Speed)
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

	task.Percent = 100
	task.Speed = 0
	task.Err = err
	p <- task

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

func mergedFiles(files []string, outputFile string, crc64 uint64) error {
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
			return fmt.Errorf("Error opening file %s: %v\n", filename, err)
		}
		defer inFile.Close()

		// 复制文件内容到输出文件
		_, err = io.Copy(outFile, inFile)
		if err != nil {
			return fmt.Errorf("Error writing to output file:%v", err)
		}
	}

	if crc64 != 0 {
		crc64check, err := crc64Checksum(outputFile)
		if err != nil {
			return err
		}

		if crc64check != crc64 {
			return fmt.Errorf("crc64Checksum failed,crc64check:%v,crc64:%v", crc64check, crc64)
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

	return fmt.Sprintf("%.2f%s", s, units[i])
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

// 计算文件的CRC64校验和
func crc64Checksum(filePath string) (uint64, error) {
	// 打开文件
	file, err := os.Open(filePath)
	if err != nil {
		return 0, err
	}
	defer file.Close()

	hasher := crc64.New(crc64.MakeTable(crc64.ECMA))

	// 读取文件内容并更新哈希
	if _, err := io.Copy(hasher, file); err != nil {
		return 0, err
	}

	// 获取最终的CRC64校验和
	return hasher.Sum64(), nil
}
