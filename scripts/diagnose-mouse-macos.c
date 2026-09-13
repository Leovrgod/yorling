// Passive mouse-event timing only: no key, text, cursor-coordinate, or clipboard capture.
#include <ApplicationServices/ApplicationServices.h>
#include <mach/mach_time.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define CAP 30000
#define YORLING_TAG 0x594F524C
struct Stats {
    uint64_t last_arrival, last_stamp;
    unsigned events, count, pauses;
    double arrival[CAP], creation[CAP], age[CAP];
};
static struct Stats yorling, other;
static CFMachPortRef observer;
static mach_timebase_info_data_t timebase;
static unsigned interruptions;
static double to_ns(uint64_t t) { return (double)t * timebase.numer / timebase.denom; }

static CGEventRef observe(CGEventTapProxy proxy, CGEventType type, CGEventRef event, void *context) {
    (void)proxy; (void)context;
    if (type == kCGEventTapDisabledByTimeout || type == kCGEventTapDisabledByUserInput) {
        interruptions++;
        CGEventTapEnable(observer, true);
        return event;
    }
    struct Stats *s = CGEventGetIntegerValueField(event, kCGEventSourceUserData) == YORLING_TAG ? &yorling : &other;
    uint64_t now = mach_absolute_time(), stamp = CGEventGetTimestamp(event);
    double gap = s->last_arrival ? to_ns(now - s->last_arrival) / 1e6 : 0;
    s->events++;
    if (s->last_arrival && gap >= 100) s->pauses++;
    if (s->last_arrival && gap < 100 && stamp >= s->last_stamp && s->count < CAP) {
        unsigned i = s->count++;
        s->arrival[i] = gap;
        s->creation[i] = (double)(stamp - s->last_stamp) / 1e6;
        s->age[i] = (to_ns(now) - (double)stamp) / 1e6;
    }
    s->last_arrival = now;
    s->last_stamp = stamp;
    return event;
}
static int compare(const void *a, const void *b) {
    double x = *(const double *)a, y = *(const double *)b;
    return (x > y) - (x < y);
}
static void quantiles(const char *name, double *values, unsigned count) {
    qsort(values, count, sizeof(double), compare);
    printf("  %s_ms median=%.3f p95=%.3f max=%.3f\n", name, values[count / 2], values[(count - 1) * 95 / 100], values[count - 1]);
}
static void report(const char *name, struct Stats *s) {
    printf("%s events=%u intervals=%u gaps_at_least_100ms=%u\n", name, s->events, s->count, s->pauses);
    if (!s->count) { puts("  Not enough movement to diagnose this source."); return; }
    quantiles("delivered_interval", s->arrival, s->count);
    quantiles("created_interval", s->creation, s->count);
    quantiles("delivery_age", s->age, s->count);
}
int main(int argc, char **argv) {
    if (argc > 1 && strcmp(argv[1], "--help") == 0) {
        puts("Usage: diagnose-mouse-macos [seconds: 1..300, default 30]\nObserves mouse movement timing only. Move with Yorling, then with your mouse/trackpad.");
        return 0;
    }
    char *end = NULL;
    long seconds = argc > 1 ? strtol(argv[1], &end, 10) : 30;
    if (argc > 2 || seconds < 1 || seconds > 300 || (end && *end)) {
        fputs("Expected a duration from 1 to 300 seconds.\n", stderr); return 2;
    }
    mach_timebase_info(&timebase);
    CGDirectDisplayID displays[32]; uint32_t count = 0;
    if (CGGetActiveDisplayList(32, displays, &count) == kCGErrorSuccess) {
        for (unsigned i = 0; i < count && i < 32; i++) {
            CGDisplayModeRef mode = CGDisplayCopyDisplayMode(displays[i]);
            if (mode) { printf("display_%u refresh_hz=%.3f (0 means unknown/variable)\n", i + 1, CGDisplayModeGetRefreshRate(mode)); CGDisplayModeRelease(mode); }
        }
    }
    observer = CGEventTapCreate(kCGSessionEventTap, kCGTailAppendEventTap, kCGEventTapOptionListenOnly,
        CGEventMaskBit(kCGEventMouseMoved), observe, NULL);
    if (!observer) {
        fputs("Observer unavailable. Check Input Monitoring/Accessibility for the terminal running this tool.\n", stderr); return 1;
    }
    CFRunLoopSourceRef source = CFMachPortCreateRunLoopSource(NULL, observer, 0);
    if (!source) { CFRelease(observer); return 1; }
    CFRunLoopAddSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
    CGEventTapEnable(observer, true);
    printf("Observing mouse-only events for %ld seconds. Move continuously with each input source in turn.\n", seconds);
    fflush(stdout);
    CFRunLoopRunInMode(kCFRunLoopDefaultMode, (double)seconds, false);
    report("yorling", &yorling);
    report("other_mouse_sources", &other);
    printf("observer_interruptions=%u\n", interruptions);
    puts("Gaps >=100 ms are counted separately: they can be idle periods or stalls.\nOther sources may include other software. Event timing does not measure visible display frames.");
    CFRunLoopRemoveSource(CFRunLoopGetCurrent(), source, kCFRunLoopCommonModes);
    CFMachPortInvalidate(observer);
    CFRelease(source); CFRelease(observer);
    return 0;
}
