// SPDX-License-Identifier: Apache-2.0

#include <errno.h>
#include <fcntl.h>
#include <stdio.h>
#include <unistd.h>

int main(void) {
    int fd = open("/etc/shadow", O_RDONLY);

    if (fd >= 0) {
        printf("the policy let the open through\n");
        close(fd);
        return 0;
    }

    printf("the policy failed the open: errno %d\n", errno);

    return 42;
}
