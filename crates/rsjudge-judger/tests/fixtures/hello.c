// SPDX-License-Identifier: Apache-2.0

#include <stdio.h>

int main(void) {
    char name[64];

    if (fgets(name, sizeof name, stdin) == NULL) {
        return 1;
    }

    printf("hello, %s", name);

    return 0;
}
