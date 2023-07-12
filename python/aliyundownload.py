#!/bin/python
#########################################################################
# File Name: down.py
# Author: happyhe
# mail: heguang@qiyi.com
# Created Time: Mon 27 Mar 2023 01:24:01 PM CST
#########################################################################

import subprocess
# 读取文件的每一行，然后执行 screen 命令将该行内容作为 shell 命令在后台执行
with open("file.txt", "r") as f:
    for line in f:
        # 使用 strip() 方法去掉行尾的换行符
        command = line.strip()
        # 使用 subprocess 模块执行 screen 命令
        subprocess.Popen(["screen", "-dmS", "my_screen", "bash", "-c", command])
