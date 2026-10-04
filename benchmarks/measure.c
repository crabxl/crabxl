/* Linux-only benchmark launcher. Not part of the Rust library or bindings.
 * A native launcher avoids inheriting the Python harness's RSS high-water mark.
 * wait4 reports the child's kernel peak RSS, including process startup.
 */
#define _GNU_SOURCE
#include <errno.h>
#include <stdio.h>
#include <stdlib.h>
#include <sys/resource.h>
#include <sys/wait.h>
#include <time.h>
#include <unistd.h>

int main(int argc, char **argv) {
    if (argc < 2) return 2;
    struct timespec start, end;
    if (clock_gettime(CLOCK_MONOTONIC, &start)) return 2;
    pid_t child = fork();
    if (child < 0) return 2;
    if (!child) {
        execvp(argv[1], argv + 1);
        perror("execvp");
        _exit(127);
    }
    int status;
    struct rusage usage;
    while (wait4(child, &status, 0, &usage) < 0) {
        if (errno != EINTR) return 2;
    }
    if (clock_gettime(CLOCK_MONOTONIC, &end)) return 2;
    double elapsed = (end.tv_sec - start.tv_sec) +
                     (end.tv_nsec - start.tv_nsec) / 1e9;
    double cpu = usage.ru_utime.tv_sec + usage.ru_utime.tv_usec / 1e6 +
                 usage.ru_stime.tv_sec + usage.ru_stime.tv_usec / 1e6;
    fprintf(stderr, "MEASURE {\"seconds\":%.9f,\"cpu_seconds\":%.9f,\"peak_rss_kib\":%ld}\n",
            elapsed, cpu, usage.ru_maxrss);
    return WIFEXITED(status) ? WEXITSTATUS(status) : 128 + WTERMSIG(status);
}
