/*************************************************************************
    > File Name: main.c
    > Author: happyhe
    > Mail: heguang@qiyi.com
    > Created Time: Mon 27 Mar 2023 06:23:50 PM CST
 ************************************************************************/

#include <stdio.h>
#include <stdio.h>
#include <string.h>
#include <unistd.h>
#define MAX_LINE_LENGTH 1024
int main(int argc, char *argv[]) {
    char *filename = "file.txt";
    char line[MAX_LINE_LENGTH];
    FILE *file = fopen(filename, "r");
    if (file == NULL) {
        perror("Failed to open file");
        exit(0);
    }
    while (fgets(line, sizeof(line), file)) {
        // 去掉行尾的换行符
        line[strcspn(line, "\n")] = 0;
        // 组装命令
        char command[MAX_LINE_LENGTH + 20];
        sprintf(command, "screen -dmS my_screen bash -c \"%s\"", line);
        // 执行命令
        system(command);
        usleep(100000); // 等待100毫秒，避免同时启动过多的screen会话
    }
    fclose(file);
    return 0;
}
