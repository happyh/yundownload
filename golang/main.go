package main

import (
	"bufio"
	"fmt"
	"os"
	"os/exec"
)

func main() {
	// 打开要读取的文件
	file, err := os.Open("file.txt")
	if err != nil {
		fmt.Println(err)
		return
	}
	defer file.Close()
	// 创建一个 bufio.Scanner 对象用于逐行读取文件内容
	scanner := bufio.NewScanner(file)
	// 逐行读取文件内容，并使用 screen 命令将该行内容作为 shell 命令在后台执行
	for scanner.Scan() {
		// 获取当前行的内容，并去掉行尾的换行符
		command := scanner.Text()
		// 使用 exec.Command() 函数创建一个 screen 命令的执行命令
		cmd := exec.Command("screen", "-dmS", "my_screen", "bash", "-c", command)
		// 执行命令
		err := cmd.Run()
		if err != nil {
			fmt.Println(err)
			return
		}
	}
	// 检查是否有错误发生
	if err := scanner.Err(); err != nil {
		fmt.Println(err)
		return
	}
}
