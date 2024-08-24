#!/bin/python3
#########################################################################
# File Name: down.py
# Author: happyhe
# mail: heguang@qiyi.com
# Created Time: Mon 27 Mar 2023 01:24:01 PM CST
#########################################################################
import concurrent.futures
import datetime
import math
import sys
import time

import argparse
import re  
import subprocess
import requests
import os

def extract_urls_and_headers(ef2_content):  
    urls_with_headers = []  
    pattern = re.compile(r'<\n(https?://.*?)\n(.*?)\n>', re.DOTALL)  
    matches = pattern.findall(ef2_content)  
      
    for url, headers_raw in matches:  
        headers = {}  
        for line in headers_raw.strip().split('\n'):  
            key, value = line.split(': ', 1)  
            headers[key.lower()] = value  
        urls_with_headers.append((url, headers))  
      
    return urls_with_headers


from urllib.parse import unquote, urlparse


def download_file_with_resume(url, headers,save_dir,savefilename):
    savefullfilename = os.path.join(save_dir, savefilename)
    start_position = 0
    if savefilename and os.path.exists(savefullfilename):
        start_position = os.path.getsize(savefullfilename)
        print(f"File {savefilename} already exists. Resuming download from byte {start_position}...")
        headers['Range'] = f'bytes={start_position}-'
    response = requests.get(url, headers=headers,stream=True,allow_redirects=True)

    result = ""
    if response.status_code in [200, 206]:
        if 'Content-Range' in response.headers:
            content_range = response.headers['Content-Range']
            file_size = int(content_range.split('/')[-1])  # 分割并获取大小值
        elif 'Content-Length' in response.headers:
            file_size = int(response.headers['Content-Length'])  # 直接获取大小值
        if start_position >= file_size :
            return (f"已经下载完成 {savefullfilename}. localfilesize:{start_position},remotefilesize:{file_size}")

        content_size = int(response.headers.get('Content-Length', 0))
        if not savefilename:
            if 'content-disposition' in response.headers:
                content_disposition = response.headers['content-disposition']
                filename_match = re.search(r'filename\*=UTF-8\'\'(.+)', content_disposition)
                if filename_match:
                    savefilename = unquote(filename_match.group(1))

            if not savefilename:
                return (f"Failed to download {url}. 获取文件名失败")

            savefilename = sanitize_filename(savefilename)
            savefullfilename = os.path.join(save_dir, savefilename)

            if os.path.exists(savefullfilename):
                start_position = os.path.getsize(savefullfilename)
                if start_position > 0:
                    if start_position >= file_size :

                        return (f"已经下载完成 {savefullfilename}. localfilesize:{start_position},remotefilesize:{file_size}")
                    print(f"File {savefullfilename} already exists. Resuming download from byte {start_position}...")
                    headers['Range']=f'bytes={start_position}-'
                    response = requests.get(url, headers=headers, stream=True, allow_redirects=True)

        downloaded_size = start_position
        last_printed_time = time.time()
        last_receive = 0
        time_interval = 1
        if response.status_code in [200, 206]:
            with open(savefullfilename, 'ab') as f:
                for chunk in response.iter_content(chunk_size=1*1024*1024):
                    if chunk:
                        f.write(chunk)
                        downloaded_size += len(chunk)
                        last_receive += len(chunk)
                        current_time = time.time()
                        if current_time - last_printed_time > time_interval:
                            completed = downloaded_size / content_size*100
                            speed = last_receive / (current_time-last_printed_time)
                            lefttime = "NaN" if speed == 0 else str(datetime.timedelta(seconds=int((content_size - downloaded_size) / speed)))
                            print(f"{savefilename} Download progress: {completed:.2f}%,speed:{human_size(speed)},leftime:{lefttime},{human_size(downloaded_size)}/{human_size(content_size)}")
                            last_printed_time = current_time
                            last_receive = 0

            result = (f"Download of {savefullfilename} completed.")
        else:
            result = (f"Failed to resume download {url}. Status code: {response.status_code}")
    else:
        result = (f"Failed to download {url}. Status code: {response.status_code}")

    print(result)
    return result

def human_size(size_bytes):
    if size_bytes == 0:
        return "0B"

    size_name = ("B", "KB", "MB", "GB", "TB", "PB", "EB", "ZB", "YB")
    i = int(math.floor(math.log(size_bytes, 1024)))
    p = math.pow(1024, i)
    s = round(size_bytes / p, 2)

    return "%s %s" % (s, size_name[i])
def sanitize_filename(filename):
    # 清理文件名中的非法字符
    # 这里可以根据需要添加更多的非法字符替换逻辑
    for char in '/\\:*?"<>|':
        filename = filename.replace(char, '_')
    return filename

def main(ef2_file):  
    try:  
        with open(ef2_file, 'r') as file:  
            ef2_content = file.read()  
            urls_with_headers = extract_urls_and_headers(ef2_content)

            # 创建一个线程池执行器，可以根据需要调整max_workers的数量
            with concurrent.futures.ThreadPoolExecutor(max_workers=len(urls_with_headers)) as executor:
                # 使用executor.submit提交任务到线程池
                futures = [executor.submit(download_file_with_resume, url, headers, ".", "")
                           for url, headers in urls_with_headers]

                #等待所有任务完成
                concurrent.futures.wait(futures)

                for future in concurrent.futures.as_completed(futures):
                    try:
                        result = future.result()  # 处理返回结果，如果有的话
                        print(f"下载文件结果: {result}")
                    except Exception as e:
                        print(f"下载文件时发生错误: {e}")
    except FileNotFoundError:  
        print(f"文件 {ef2_file} 未找到。")  
    except Exception as e:  
        print(f"发生错误: {e}")  
  
if __name__ == "__main__":
    parser = argparse.ArgumentParser(description='从 ef2 文件中解析下载地址并使用 curl 下载。')  
    parser.add_argument('ef2_file', type=str, help='ef2 格式文件的名称')
    parser.add_argument('-s', '--noscreen',action='store_true', help='是否关闭screen模式',default=False)
    args = parser.parse_args()

    if args.noscreen:
        main(args.ef2_file)
    else:
        print("当前目录：",os.getcwd(), ",下载文件：",args.ef2_file,"已经后台screen执行，可screen -r进入查看下载进度")
        command = sys.argv[0] + " " + args.ef2_file + " --noscreen"
        subprocess.Popen(["screen", "-dmS", "my_screen", "bash", "-c", command])
