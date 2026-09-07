// Interpose hidapi's write/read so the vendor app's traffic can be read off
// the wire. Logging only: every call is forwarded unchanged.
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <time.h>

typedef struct hid_device_ hid_device;
extern int hid_write(hid_device *dev, const unsigned char *data, size_t length);
extern int hid_read_timeout(hid_device *dev, unsigned char *data, size_t length, int ms);

static FILE *logf(void) {
    static FILE *f;
    if (!f) {
        const char *p = getenv("HIDLOG");
        f = fopen(p ? p : "/tmp/hidlog.txt", "a");
        if (f) setvbuf(f, NULL, _IOLBF, 0);
    }
    return f;
}

static void dump(const char *tag, const unsigned char *d, size_t n) {
    FILE *f = logf();
    if (!f) return;
    size_t show = n > 24 ? 24 : n;
    fprintf(f, "%s len=%zu:", tag, n);
    for (size_t i = 0; i < show; i++) fprintf(f, " %02x", d[i]);
    fprintf(f, "%s\n", n > show ? " ..." : "");
}

static int my_hid_write(hid_device *dev, const unsigned char *data, size_t length) {
    dump("WRITE", data, length);
    return hid_write(dev, data, length);
}

static int my_hid_read_timeout(hid_device *dev, unsigned char *data, size_t length, int ms) {
    int r = hid_read_timeout(dev, data, length, ms);
    if (r > 0) dump("READ ", data, (size_t)r);
    return r;
}

__attribute__((used)) static struct {
    const void *replacement;
    const void *replacee;
} interposers[] __attribute__((section("__DATA,__interpose"))) = {
    { (const void *)my_hid_write,        (const void *)hid_write },
    { (const void *)my_hid_read_timeout, (const void *)hid_read_timeout },
};
