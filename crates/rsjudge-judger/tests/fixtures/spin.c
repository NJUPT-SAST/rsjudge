// SPDX-License-Identifier: Apache-2.0

int main(void) {
    volatile unsigned long long counter = 0;

    for (;;) {
        counter += 1;

        // The comparison keeps the counter live, so the compiler does not
        // warn about a value that is set but never used.
        if (counter == 0) {
            return 1;
        }
    }
}
